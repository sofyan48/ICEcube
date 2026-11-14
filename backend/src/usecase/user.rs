//! Users (IcecubeUser CRs) and their RoleBindings / ClusterRoleBindings.
//!
//! The user's spec is the source of truth. The bindings that exist in the
//! cluster are always derived from it: direct `bindings` plus the bindings of
//! every group in `groups`. `reconcile` makes the cluster match.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use k8s_openapi::api::{
    core::v1::Namespace,
    rbac::v1::{ClusterRoleBinding, RoleBinding, RoleRef, Subject},
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::{
    Api, Client, Resource, ResourceExt,
    api::{DeleteParams, ListParams, ObjectMeta, Patch, PatchParams, PostParams},
};

use crate::{
    library::{
        error::{AppError, AppResult, is_not_found},
        kubeconfig_store,
        model::*,
        state::AppState,
    },
    usecase::{group as groups, template as templates},
};

fn user_selector(user: &str) -> String {
    format!("{LABEL_MANAGED_BY}={MANAGER},{LABEL_USER}={user}")
}

fn binding_labels(user: &str, template: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        (LABEL_MANAGED_BY.to_string(), MANAGER.to_string()),
        (LABEL_USER.to_string(), user.to_string()),
        (LABEL_TEMPLATE_REF.to_string(), template.to_string()),
    ])
}

fn subject(user: &str) -> Subject {
    Subject {
        kind: "User".into(),
        name: user.into(),
        api_group: Some(RBAC_GROUP.into()),
        namespace: None,
    }
}

fn role_ref(template: &str) -> RoleRef {
    RoleRef {
        api_group: Some(RBAC_GROUP.into()),
        kind: "ClusterRole".into(),
        name: template_cluster_role(template),
    }
}

/// IcecubeUser is cluster scoped, so it can own both RoleBindings and ClusterRoleBindings.
fn owner(user: &IcecubeUser) -> Option<OwnerReference> {
    user.controller_owner_ref(&()).map(|mut o| {
        // Not a controller in the ownership sense; we only want cascading delete.
        o.controller = None;
        o.block_owner_deletion = None;
        o
    })
}

/// Bindings that currently exist in the cluster, grouped by user.
async fn applied_bindings(
    client: &Client,
    selector: &str,
) -> AppResult<HashMap<String, BTreeSet<BindingDto>>> {
    let lp = ListParams::default().labels(selector);
    let rbs: Api<RoleBinding> = Api::all(client.clone());
    let crbs: Api<ClusterRoleBinding> = Api::all(client.clone());
    let (rbs, crbs) = futures::try_join!(rbs.list_metadata(&lp), crbs.list_metadata(&lp))?;

    let mut out: HashMap<String, BTreeSet<BindingDto>> = HashMap::new();
    let mut add = |labels: &BTreeMap<String, String>, namespace: Option<String>| {
        if let (Some(user), Some(template)) =
            (labels.get(LABEL_USER), labels.get(LABEL_TEMPLATE_REF))
        {
            out.entry(user.clone()).or_default().insert(BindingDto {
                template: template.clone(),
                namespace,
            });
        }
    };
    for rb in &rbs.items {
        add(rb.labels(), rb.namespace());
    }
    for crb in &crbs.items {
        add(crb.labels(), None);
    }
    Ok(out)
}

fn to_dto(user: &IcecubeUser) -> UserDto {
    let mut bindings = user.spec.bindings.clone().unwrap_or_default();
    bindings.sort();
    UserDto {
        name: user.name_any(),
        description: user.spec.description.clone(),
        created_at: user
            .metadata
            .creation_timestamp
            .as_ref()
            .map(|t| t.0.to_string()),
        bindings,
        groups: user.spec.groups.clone(),
        certificates: user
            .status
            .iter()
            .flat_map(|s| s.certificates.iter().rev().cloned())
            .collect(),
    }
}

pub async fn list_crs(client: &Client) -> AppResult<Vec<IcecubeUser>> {
    let api: Api<IcecubeUser> = Api::all(client.clone());
    Ok(api.list(&ListParams::default()).await?.items)
}

pub async fn list(client: &Client) -> AppResult<Vec<UserDto>> {
    let mut out: Vec<_> = list_crs(client).await?.iter().map(to_dto).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub async fn get_cr(client: &Client, name: &str) -> AppResult<IcecubeUser> {
    let api: Api<IcecubeUser> = Api::all(client.clone());
    api.get_opt(name)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("user '{name}' not found")))
}

pub async fn get(client: &Client, name: &str) -> AppResult<UserDto> {
    Ok(to_dto(&get_cr(client, name).await?))
}

pub async fn create(client: &Client, input: CreateUserInput) -> AppResult<UserDto> {
    validate_name("user", &input.name)?;
    let bindings = validate_bindings(client, &input.bindings).await?;
    let all_groups = groups::list_crs(client).await?;
    let group_names = match input.groups {
        Some(g) => validate_groups(&all_groups, &g)?,
        None => all_groups
            .iter()
            .filter(|g| g.spec.default_for_new_users)
            .map(|g| g.name_any())
            .collect(),
    };

    let api: Api<IcecubeUser> = Api::all(client.clone());
    let mut cr = IcecubeUser::new(
        &input.name,
        IcecubeUserSpec {
            description: input.description.filter(|d| !d.trim().is_empty()),
            bindings: Some(bindings.into_iter().collect()),
            groups: group_names,
        },
    );
    cr.metadata.labels = Some(BTreeMap::from([(LABEL_MANAGED_BY.into(), MANAGER.into())]));
    let created = api
        .create(&PostParams::default(), &cr)
        .await
        .map_err(|e| match e {
            kube::Error::Api(s) if s.code == 409 => {
                AppError::Conflict(format!("user '{}' already exists", input.name))
            }
            e => e.into(),
        })?;

    reconcile(client, &created, &all_groups).await?;
    Ok(to_dto(&created))
}

pub async fn update(client: &Client, name: &str, input: UpdateUserInput) -> AppResult<UserDto> {
    get_cr(client, name).await?;
    let bindings = validate_bindings(client, &input.bindings).await?;
    let all_groups = groups::list_crs(client).await?;
    let group_names = validate_groups(&all_groups, &input.groups)?;

    let api: Api<IcecubeUser> = Api::all(client.clone());
    let patch = serde_json::json!({
        "spec": {
            "description": input.description.filter(|d| !d.trim().is_empty()),
            "bindings": bindings,
            "groups": group_names,
        }
    });
    let user = api
        .patch(name, &PatchParams::default(), &Patch::Merge(patch))
        .await?;

    reconcile(client, &user, &all_groups).await?;
    Ok(to_dto(&user))
}

pub async fn delete(state: &AppState, name: &str) -> AppResult<()> {
    let client = &state.client;
    let user = get_cr(client, name).await?;
    // Remove bindings explicitly so access is revoked immediately instead of
    // waiting for the garbage collector.
    apply(client, &user, &BTreeSet::new()).await?;
    let api: Api<IcecubeUser> = Api::all(client.clone());
    match api.delete(name, &DeleteParams::background()).await {
        Ok(_) => {}
        Err(e) if is_not_found(&e) => {}
        Err(e) => return Err(e.into()),
    }
    // Also garbage-collected through its owner reference; deleting it here
    // removes it right away.
    kubeconfig_store::delete(state, name).await
}

fn validate_groups(all: &[IcecubeGroup], names: &[String]) -> AppResult<Vec<String>> {
    let known: BTreeSet<String> = all.iter().map(|g| g.name_any()).collect();
    let mut out = BTreeSet::new();
    for n in names {
        if !known.contains(n) {
            return Err(AppError::BadRequest(format!("unknown group '{n}'")));
        }
        out.insert(n.clone());
    }
    Ok(out.into_iter().collect())
}

/// Check that every template exists, scopes fit, and namespaces exist.
pub async fn validate_bindings(
    client: &Client,
    bindings: &[BindingDto],
) -> AppResult<BTreeSet<BindingDto>> {
    let templates: HashMap<String, Scope> = templates::list(client)
        .await?
        .into_iter()
        .map(|t| (t.name, t.scope))
        .collect();
    let namespaces = namespace_set(client).await?;

    let mut out = BTreeSet::new();
    for b in bindings {
        let namespace = b.namespace.clone().filter(|n| !n.is_empty());
        let scope = templates
            .get(&b.template)
            .ok_or_else(|| AppError::BadRequest(format!("unknown template '{}'", b.template)))?;
        match (&namespace, scope) {
            (Some(_), Scope::Cluster) => {
                return Err(AppError::BadRequest(format!(
                    "template '{}' is cluster-scoped and cannot be bound to a namespace",
                    b.template
                )));
            }
            (Some(ns), _) if !namespaces.contains(ns) => {
                return Err(AppError::BadRequest(format!(
                    "namespace '{ns}' does not exist"
                )));
            }
            _ => {}
        }
        out.insert(BindingDto {
            template: b.template.clone(),
            namespace,
        });
    }
    Ok(out)
}

async fn namespace_set(client: &Client) -> AppResult<BTreeSet<String>> {
    let api: Api<Namespace> = Api::all(client.clone());
    Ok(api
        .list_metadata(&ListParams::default())
        .await?
        .items
        .into_iter()
        .map(|n| n.name_any())
        .collect())
}

/// Direct bindings plus the bindings of each group the user belongs to.
/// Unknown groups are ignored, so a deleted group simply stops granting access.
pub fn effective_bindings(user: &IcecubeUser, groups: &[IcecubeGroup]) -> BTreeSet<BindingDto> {
    let mut out: BTreeSet<BindingDto> = user.spec.bindings.iter().flatten().cloned().collect();
    for g in groups {
        if user.spec.groups.contains(&g.name_any()) {
            out.extend(g.spec.bindings.iter().cloned());
        }
    }
    out
}

/// Recompute the user's effective bindings and apply them to the cluster.
pub async fn reconcile(
    client: &Client,
    user: &IcecubeUser,
    groups: &[IcecubeGroup],
) -> AppResult<()> {
    apply(client, user, &effective_bindings(user, groups)).await
}

/// Make the user's bindings in the cluster match `desired` exactly.
async fn apply(
    client: &Client,
    user: &IcecubeUser,
    desired: &BTreeSet<BindingDto>,
) -> AppResult<()> {
    let name = user.name_any();
    let current = applied_bindings(client, &user_selector(&name))
        .await?
        .remove(&name)
        .unwrap_or_default();
    let owner = owner(user);
    let pp = PatchParams::apply(MANAGER).force();

    let missing: Vec<_> = desired.difference(&current).collect();
    // A group can still reference a namespace that was deleted later; skip it
    // instead of failing the whole user.
    let namespaces = if missing.iter().any(|b| b.namespace.is_some()) {
        namespace_set(client).await?
    } else {
        BTreeSet::new()
    };

    for b in missing {
        if let Some(ns) = &b.namespace
            && !namespaces.contains(ns)
        {
            tracing::warn!(user = %name, template = %b.template, namespace = %ns, "namespace missing, binding skipped");
            continue;
        }
        let meta = ObjectMeta {
            name: Some(binding_name(&name, &b.template)),
            namespace: b.namespace.clone(),
            labels: Some(binding_labels(&name, &b.template)),
            owner_references: owner.clone().map(|o| vec![o]),
            ..Default::default()
        };
        match &b.namespace {
            Some(ns) => {
                let api: Api<RoleBinding> = Api::namespaced(client.clone(), ns);
                let rb = RoleBinding {
                    metadata: meta,
                    role_ref: role_ref(&b.template),
                    subjects: Some(vec![subject(&name)]),
                };
                api.patch(&binding_name(&name, &b.template), &pp, &Patch::Apply(rb))
                    .await?;
            }
            None => {
                let api: Api<ClusterRoleBinding> = Api::all(client.clone());
                let crb = ClusterRoleBinding {
                    metadata: meta,
                    role_ref: role_ref(&b.template),
                    subjects: Some(vec![subject(&name)]),
                };
                api.patch(&binding_name(&name, &b.template), &pp, &Patch::Apply(crb))
                    .await?;
            }
        }
        tracing::info!(user = %name, template = %b.template, namespace = ?b.namespace, "binding created");
    }

    for b in current.difference(desired) {
        let dp = DeleteParams::default();
        let res = match &b.namespace {
            Some(ns) => {
                let api: Api<RoleBinding> = Api::namespaced(client.clone(), ns);
                api.delete(&binding_name(&name, &b.template), &dp)
                    .await
                    .map(|_| ())
            }
            None => {
                let api: Api<ClusterRoleBinding> = Api::all(client.clone());
                api.delete(&binding_name(&name, &b.template), &dp)
                    .await
                    .map(|_| ())
            }
        };
        match res {
            Ok(()) => {
                tracing::info!(user = %name, template = %b.template, namespace = ?b.namespace, "binding removed")
            }
            Err(e) if is_not_found(&e) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Users created before groups existed have no `spec.bindings`; their grants
/// only live in the cluster. Copy them into the spec once so nothing is lost.
pub async fn migrate_legacy(client: &Client) -> AppResult<()> {
    let api: Api<IcecubeUser> = Api::all(client.clone());
    let legacy: Vec<_> = list_crs(client)
        .await?
        .into_iter()
        .filter(|u| u.spec.bindings.is_none())
        .collect();
    if legacy.is_empty() {
        return Ok(());
    }
    let mut applied = applied_bindings(
        client,
        &format!("{LABEL_MANAGED_BY}={MANAGER},{LABEL_USER}"),
    )
    .await?;
    for u in legacy {
        let name = u.name_any();
        let bindings: Vec<_> = applied
            .remove(&name)
            .unwrap_or_default()
            .into_iter()
            .collect();
        api.patch(
            &name,
            &PatchParams::default(),
            &Patch::Merge(serde_json::json!({ "spec": { "bindings": bindings } })),
        )
        .await?;
        tracing::info!(user = %name, count = bindings.len(), "migrated bindings into user spec");
    }
    Ok(())
}

pub async fn list_namespaces(client: &Client) -> AppResult<Vec<String>> {
    Ok(namespace_set(client).await?.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(template: &str, ns: Option<&str>) -> BindingDto {
        BindingDto {
            template: template.into(),
            namespace: ns.map(Into::into),
        }
    }

    fn group(name: &str, bindings: Vec<BindingDto>) -> IcecubeGroup {
        IcecubeGroup::new(
            name,
            IcecubeGroupSpec {
                description: None,
                bindings,
                default_for_new_users: false,
            },
        )
    }

    #[test]
    fn effective_bindings_merge_direct_and_groups() {
        let user = IcecubeUser::new(
            "jane",
            IcecubeUserSpec {
                description: None,
                bindings: Some(vec![b("developer", Some("team-a")), b("viewer", None)]),
                groups: vec!["base".into(), "deleted".into()],
            },
        );
        let groups = vec![
            group("base", vec![b("viewer", None), b("namespace-lister", None)]),
            group("other", vec![b("cluster-viewer", None)]),
        ];
        let got: Vec<_> = effective_bindings(&user, &groups).into_iter().collect();
        assert_eq!(
            got,
            vec![
                b("developer", Some("team-a")),
                b("namespace-lister", None),
                b("viewer", None)
            ]
        );
    }
}
