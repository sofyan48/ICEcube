//! What the cluster actually serves, so the template editor offers real API
//! groups, resources (including subresources such as `pods/log`) and verbs
//! instead of free text. Same source as `kubectl api-resources`.

use std::collections::{BTreeMap, BTreeSet};

use kube::Client;

use crate::library::{
    error::AppResult,
    model::{ApiGroupDto, ApiResourceDto},
};

/// RBAC verbs that discovery does not list because they are checked by
/// authorizers, not served as API operations.
fn extra_verbs(group: &str, resource: &str) -> &'static [&'static str] {
    match (group, resource) {
        ("rbac.authorization.k8s.io", "roles" | "clusterroles") => &["bind", "escalate"],
        ("", "serviceaccounts") => &["impersonate"],
        ("certificates.k8s.io", "certificatesigningrequests/approval") => &["approve"],
        ("policy", "podsecuritypolicies") => &["use"],
        _ => &[],
    }
}

/// Resources that only exist for RBAC checks and never appear in discovery.
fn virtual_resources() -> Vec<(&'static str, ApiResourceDto)> {
    let r = |name: &str, verbs: &[&str]| ApiResourceDto {
        name: name.into(),
        kind: String::new(),
        namespaced: false,
        verbs: verbs.iter().map(|v| v.to_string()).collect(),
        rbac_only: true,
    };
    vec![
        ("", r("users", &["impersonate"])),
        ("", r("groups", &["impersonate"])),
        ("", r("uids", &["impersonate"])),
        (
            "authentication.k8s.io",
            r("userextras/scopes", &["impersonate"]),
        ),
        (
            "certificates.k8s.io",
            r("signers", &["approve", "sign", "attest"]),
        ),
    ]
}

/// Every API group with the union of its resources over all served versions.
/// Groups whose discovery fails (e.g. an aggregated API whose backend is down)
/// are skipped so one broken APIService does not hide everything else.
pub async fn list(client: &Client) -> AppResult<Vec<ApiGroupDto>> {
    let (core, groups) =
        futures::try_join!(client.list_core_api_versions(), client.list_api_groups())?;

    let mut targets: Vec<(String, String)> = core
        .versions
        .into_iter()
        .map(|v| (String::new(), v))
        .collect();
    for g in groups.groups {
        for v in g.versions {
            targets.push((g.name.clone(), v.group_version));
        }
    }

    let lists = futures::future::join_all(targets.iter().map(|(group, gv)| async move {
        let res = if group.is_empty() {
            client.list_core_api_resources(gv).await
        } else {
            client.list_api_group_resources(gv).await
        };
        (group, gv, res)
    }))
    .await;

    let mut merged: BTreeMap<String, BTreeMap<String, (ApiResourceDto, BTreeSet<String>)>> =
        BTreeMap::new();
    for (group, gv, res) in lists {
        let list = match res {
            Ok(list) => list,
            Err(e) => {
                tracing::debug!(group_version = %gv, error = %e, "discovery failed, group skipped");
                continue;
            }
        };
        let resources = merged.entry(group.clone()).or_default();
        for r in list.resources {
            let entry = resources.entry(r.name.clone()).or_insert_with(|| {
                (
                    ApiResourceDto {
                        name: r.name.clone(),
                        kind: r.kind.clone(),
                        namespaced: r.namespaced,
                        verbs: vec![],
                        rbac_only: false,
                    },
                    BTreeSet::new(),
                )
            });
            entry.1.extend(r.verbs);
            entry
                .1
                .extend(extra_verbs(group, &r.name).iter().map(|v| v.to_string()));
        }
    }
    for (group, r) in virtual_resources() {
        if let Some(resources) = merged.get_mut(group) {
            let verbs = r.verbs.iter().cloned().collect();
            resources.entry(r.name.clone()).or_insert((r, verbs));
        }
    }

    Ok(merged
        .into_iter()
        .map(|(name, resources)| ApiGroupDto {
            name,
            resources: resources
                .into_values()
                .map(|(mut r, verbs)| {
                    r.verbs = verbs.into_iter().collect();
                    r
                })
                .collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rbac_only_verbs_cover_privileged_resources() {
        assert_eq!(
            extra_verbs("rbac.authorization.k8s.io", "clusterroles"),
            &["bind", "escalate"]
        );
        assert!(extra_verbs("apps", "deployments").is_empty());
        assert!(
            virtual_resources()
                .iter()
                .any(|(g, r)| g.is_empty() && r.name == "users")
        );
    }
}
