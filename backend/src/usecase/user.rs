//! Users (IcecubeUser CRs) and their RoleBindings / ClusterRoleBindings.
//!
//! The user's spec is the source of truth. The bindings that exist in the
//! cluster are always derived from it: direct `bindings` plus the bindings of
//! every group in `groups`. The API only writes the IcecubeUser; the controller
//! (`usecase::controller`) calls `reconcile` to make the cluster match, so
//! users applied with kubectl/GitOps behave exactly like ones made in the UI.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use k8s_openapi::api::{
    core::v1::{Namespace, ServiceAccount},
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
        tombstone,
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

fn user_labels(user: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        (LABEL_MANAGED_BY.to_string(), MANAGER.to_string()),
        (LABEL_USER.to_string(), user.to_string()),
    ])
}

/// Who the bindings grant access to, depending on the user's credential kind.
fn subject(state: &AppState, user: &IcecubeUser) -> Subject {
    let name = user.name_any();
    match user.spec.credential {
        CredentialKind::Certificate => Subject {
            kind: "User".into(),
            name,
            api_group: Some(RBAC_GROUP.into()),
            namespace: None,
        },
        CredentialKind::ServiceAccount => Subject {
            kind: "ServiceAccount".into(),
            name: service_account_name(&name),
            api_group: None,
            namespace: Some(state.namespace.clone()),
        },
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
        credential: user.spec.credential,
        certificates: user
            .status
            .iter()
            .flat_map(|s| s.certificates.iter().rev().cloned())
            .collect(),
        blocked: user.status.as_ref().and_then(|s| s.blocked.clone()),
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

pub async fn create(state: &AppState, input: CreateUserInput) -> AppResult<UserDto> {
    let client = &state.client;
    validate_name("user", &input.name)?;
    let credential = input.credential.unwrap_or_default();
    // Old certificates only match `User` subjects, so ServiceAccount users are safe.
    if credential == CredentialKind::Certificate
        && !input.allow_name_reuse
        && let Some(until) = tombstone::active(state, &input.name).await?
    {
        return Err(AppError::Conflict(format!(
            "username '{}' was deleted recently and certificates issued to it stay valid until {until}; \
             pick another name or wait until then",
            input.name
        )));
    }
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
            credential,
        },
    );
    cr.metadata.labels = Some(BTreeMap::from([(LABEL_MANAGED_BY.into(), MANAGER.into())]));
    if input.allow_name_reuse {
        cr.metadata.annotations = Some(BTreeMap::from([(
            ANNO_ALLOW_NAME_REUSE.into(),
            "true".into(),
        )]));
    }
    let created = api
        .create(&PostParams::default(), &cr)
        .await
        .map_err(|e| match e {
            kube::Error::Api(s) if s.code == 409 => {
                AppError::Conflict(format!("user '{}' already exists", input.name))
            }
            e => e.into(),
        })?;
    // The controller creates the bindings (and ServiceAccount) within moments.
    Ok(to_dto(&created))
}

pub async fn update(client: &Client, name: &str, input: UpdateUserInput) -> AppResult<UserDto> {
    get_cr(client, name).await?;
    let bindings = validate_bindings(client, &input.bindings).await?;
    let all_groups = groups::list_crs(client).await?;
    let group_names = validate_groups(&all_groups, &input.groups)?;

    let api: Api<IcecubeUser> = Api::all(client.clone());
    let mut spec = serde_json::json!({
        "description": input.description.filter(|d| !d.trim().is_empty()),
        "bindings": bindings,
        "groups": group_names,
    });
    if let Some(kind) = input.credential {
        spec["credential"] = serde_json::json!(kind);
    }
    let user = api
        .patch(
            name,
            &PatchParams::default(),
            &Patch::Merge(serde_json::json!({ "spec": spec })),
        )
        .await?;
    Ok(to_dto(&user))
}

/// Deletes the IcecubeUser. The controller's finalizer revokes the bindings,
/// removes the ServiceAccount and stored kubeconfig, and remembers the name
/// while certificates issued to it are still valid.
pub async fn delete(state: &AppState, name: &str) -> AppResult<()> {
    get_cr(&state.client, name).await?;
    let api: Api<IcecubeUser> = Api::all(state.client.clone());
    match api.delete(name, &DeleteParams::background()).await {
        Ok(_) => Ok(()),
        Err(e) if is_not_found(&e) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Invalidate every token issued to a ServiceAccount user right away by
/// deleting its ServiceAccount (bound tokens carry the ServiceAccount's UID).
/// The controller recreates the ServiceAccount with a new UID.
pub async fn revoke(state: &AppState, name: &str) -> AppResult<UserDto> {
    let user = get_cr(&state.client, name).await?;
    if user.spec.credential != CredentialKind::ServiceAccount {
        return Err(AppError::BadRequest(format!(
            "user '{name}' uses client certificates, which Kubernetes cannot revoke; \
             switch the user to ServiceAccount credentials (old certificates then lose all access) or delete the user"
        )));
    }
    delete_service_account(state, name).await?;
    if let Err(e) = kubeconfig_store::delete(state, name).await {
        tracing::warn!(user = name, error = %e, "could not remove stored kubeconfig");
    }

    let api: Api<IcecubeUser> = Api::all(state.client.clone());
    let current = api.get_status(name).await?;
    let now = jiff::Timestamp::now();
    let stamp = now
        .round(jiff::Unit::Second)
        .map_err(AppError::internal)?
        .to_string();
    let mut certificates = current.status.map(|s| s.certificates).unwrap_or_default();
    for c in &mut certificates {
        let live = c
            .expires_at
            .parse::<jiff::Timestamp>()
            .is_ok_and(|t| t > now);
        if c.kind == CredentialKind::ServiceAccount && live && c.revoked_at.is_none() {
            c.revoked_at = Some(stamp.clone());
        }
    }
    let updated = api
        .patch_status(
            name,
            &PatchParams::default(),
            &Patch::Merge(serde_json::json!({ "status": { "certificates": certificates } })),
        )
        .await?;
    tracing::info!(user = name, "service account tokens revoked");
    Ok(to_dto(&updated))
}

/// Latest expiry of any client certificate issued to this user.
pub fn cert_valid_until(user: &IcecubeUser) -> Option<jiff::Timestamp> {
    let status = user.status.as_ref()?;
    status
        .certificates
        .iter()
        .filter(|c| c.kind == CredentialKind::Certificate)
        .map(|c| c.expires_at.as_str())
        .chain(status.certificates_valid_until.as_deref())
        .filter_map(|s| s.parse::<jiff::Timestamp>().ok())
        .max()
}

/// Create or update the ServiceAccount of a token-based user.
pub async fn ensure_service_account(state: &AppState, user: &IcecubeUser) -> AppResult<()> {
    let name = user.name_any();
    let sa_name = service_account_name(&name);
    let sa = ServiceAccount {
        metadata: ObjectMeta {
            name: Some(sa_name.clone()),
            namespace: Some(state.namespace.clone()),
            labels: Some(user_labels(&name)),
            owner_references: owner(user).map(|o| vec![o]),
            ..Default::default()
        },
        // Only tokens minted through ICEcube; never mounted into pods.
        automount_service_account_token: Some(false),
        ..Default::default()
    };
    let api: Api<ServiceAccount> = Api::namespaced(state.client.clone(), &state.namespace);
    api.patch(
        &sa_name,
        &PatchParams::apply(MANAGER).force(),
        &Patch::Apply(&sa),
    )
    .await?;
    Ok(())
}

/// Remove the user's ServiceAccount, invalidating all of its tokens. Missing is fine.
pub async fn delete_service_account(state: &AppState, user: &str) -> AppResult<()> {
    let api: Api<ServiceAccount> = Api::namespaced(state.client.clone(), &state.namespace);
    match api
        .delete(&service_account_name(user), &DeleteParams::default())
        .await
    {
        Ok(_) => {
            tracing::info!(user, "service account removed");
            Ok(())
        }
        Err(e) if is_not_found(&e) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Take away everything the user can access: bindings and ServiceAccount.
pub async fn revoke_all(state: &AppState, user: &IcecubeUser) -> AppResult<()> {
    apply(state, user, &BTreeSet::new()).await?;
    delete_service_account(state, &user.name_any()).await
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
    state: &AppState,
    user: &IcecubeUser,
    groups: &[IcecubeGroup],
) -> AppResult<()> {
    apply(state, user, &effective_bindings(user, groups)).await
}

/// Make the user's bindings in the cluster match `desired` exactly.
///
/// Every desired binding is server-side applied, not only missing ones, so
/// manual edits (e.g. changed subjects) are reverted too. Applying an
/// unchanged object is a no-op and does not emit a watch event.
async fn apply(
    state: &AppState,
    user: &IcecubeUser,
    desired: &BTreeSet<BindingDto>,
) -> AppResult<()> {
    let client = &state.client;
    let name = user.name_any();
    let current = applied_bindings(client, &user_selector(&name))
        .await?
        .remove(&name)
        .unwrap_or_default();
    let owner = owner(user);
    let subject = subject(state, user);
    let pp = PatchParams::apply(MANAGER).force();

    // A group can still reference a namespace that was deleted later; skip it
    // instead of failing the whole user.
    let namespaces = if desired.iter().any(|b| b.namespace.is_some()) {
        namespace_set(client).await?
    } else {
        BTreeSet::new()
    };

    for b in desired {
        if let Some(ns) = &b.namespace
            && !namespaces.contains(ns)
        {
            tracing::debug!(user = %name, template = %b.template, namespace = %ns, "namespace missing, binding skipped");
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
                    subjects: Some(vec![subject.clone()]),
                };
                api.patch(&binding_name(&name, &b.template), &pp, &Patch::Apply(rb))
                    .await?;
            }
            None => {
                let api: Api<ClusterRoleBinding> = Api::all(client.clone());
                let crb = ClusterRoleBinding {
                    metadata: meta,
                    role_ref: role_ref(&b.template),
                    subjects: Some(vec![subject.clone()]),
                };
                api.patch(&binding_name(&name, &b.template), &pp, &Patch::Apply(crb))
                    .await?;
            }
        }
        if !current.contains(b) {
            tracing::info!(user = %name, template = %b.template, namespace = ?b.namespace, "binding created");
        }
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

    fn issued(kind: CredentialKind, expires_at: &str) -> IssuedCertificate {
        IssuedCertificate {
            serial: expires_at.into(),
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: expires_at.into(),
            namespace: "default".into(),
            requested_days: 30,
            kind,
            revoked_at: None,
        }
    }

    #[test]
    fn cert_valid_until_ignores_tokens_and_keeps_pruned_expiry() {
        let mut user = IcecubeUser::new(
            "jane",
            IcecubeUserSpec {
                description: None,
                bindings: Some(vec![]),
                groups: vec![],
                credential: CredentialKind::Certificate,
            },
        );
        assert_eq!(cert_valid_until(&user), None);

        user.status = Some(IcecubeUserStatus {
            certificates: vec![
                issued(CredentialKind::Certificate, "2026-02-01T00:00:00Z"),
                issued(CredentialKind::ServiceAccount, "2027-01-01T00:00:00Z"),
            ],
            certificates_valid_until: Some("2026-03-01T00:00:00Z".into()),
            blocked: None,
        });
        assert_eq!(
            cert_valid_until(&user).map(|t| t.to_string()),
            Some("2026-03-01T00:00:00Z".into())
        );
    }

    #[test]
    fn effective_bindings_merge_direct_and_groups() {
        let user = IcecubeUser::new(
            "jane",
            IcecubeUserSpec {
                description: None,
                bindings: Some(vec![b("developer", Some("team-a")), b("viewer", None)]),
                groups: vec!["base".into(), "deleted".into()],
                credential: CredentialKind::Certificate,
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
