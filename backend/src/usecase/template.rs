//! Permission templates, stored as labelled ClusterRoles.

use std::collections::BTreeMap;

use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, PolicyRule, RoleBinding};
use kube::{
    Api, Client,
    api::{DeleteParams, ListParams, ObjectMeta, Patch, PatchParams, PostParams},
};

use crate::library::{
    error::{AppError, AppResult, is_not_found},
    model::*,
};

fn template_selector() -> String {
    format!("{LABEL_MANAGED_BY}={MANAGER},{LABEL_TEMPLATE}")
}

pub fn to_dto(role: &ClusterRole) -> Option<TemplateDto> {
    let labels = role.metadata.labels.as_ref()?;
    let name = labels.get(LABEL_TEMPLATE)?.clone();
    let annotations = role.metadata.annotations.clone().unwrap_or_default();
    let scope = annotations
        .get(ANNO_SCOPE)
        .and_then(|s| Scope::parse(s))
        .unwrap_or(Scope::Namespaced);
    Some(TemplateDto {
        cluster_role: role.metadata.name.clone().unwrap_or_default(),
        builtin: labels.get(LABEL_BUILTIN).is_some_and(|v| v == "true"),
        description: annotations
            .get(ANNO_DESCRIPTION)
            .cloned()
            .unwrap_or_default(),
        rules: role.rules.clone().unwrap_or_default(),
        name,
        scope,
    })
}

fn build(input: &TemplateInput, builtin: bool) -> ClusterRole {
    let mut labels = BTreeMap::from([
        (LABEL_MANAGED_BY.to_string(), MANAGER.to_string()),
        (LABEL_TEMPLATE.to_string(), input.name.clone()),
    ]);
    if builtin {
        labels.insert(LABEL_BUILTIN.to_string(), "true".to_string());
    }
    ClusterRole {
        metadata: ObjectMeta {
            name: Some(template_cluster_role(&input.name)),
            labels: Some(labels),
            annotations: Some(BTreeMap::from([
                (ANNO_SCOPE.to_string(), input.scope.as_str().to_string()),
                (ANNO_DESCRIPTION.to_string(), input.description.clone()),
            ])),
            ..Default::default()
        },
        rules: Some(input.rules.clone()),
        aggregation_rule: None,
    }
}

fn validate(input: &TemplateInput) -> AppResult<()> {
    validate_name("template", &input.name)?;
    if input.rules.is_empty() {
        return Err(AppError::BadRequest(
            "a template needs at least one rule".into(),
        ));
    }
    for (i, rule) in input.rules.iter().enumerate() {
        if rule.verbs.is_empty() {
            return Err(AppError::BadRequest(format!(
                "rule #{} has no verbs",
                i + 1
            )));
        }
        let has_resources = rule.resources.as_ref().is_some_and(|r| !r.is_empty());
        let has_urls = rule
            .non_resource_urls
            .as_ref()
            .is_some_and(|r| !r.is_empty());
        if !has_resources && !has_urls {
            return Err(AppError::BadRequest(format!(
                "rule #{} needs resources or nonResourceURLs",
                i + 1
            )));
        }
        if has_urls && input.scope == Scope::Namespaced {
            return Err(AppError::BadRequest(
                "nonResourceURLs are only allowed in cluster-scoped templates".into(),
            ));
        }
    }
    Ok(())
}

pub async fn list(client: &Client) -> AppResult<Vec<TemplateDto>> {
    let api: Api<ClusterRole> = Api::all(client.clone());
    let roles = api
        .list(&ListParams::default().labels(&template_selector()))
        .await?;
    let mut out: Vec<_> = roles.items.iter().filter_map(to_dto).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub async fn get(client: &Client, name: &str) -> AppResult<TemplateDto> {
    let api: Api<ClusterRole> = Api::all(client.clone());
    match api.get_opt(&template_cluster_role(name)).await? {
        Some(role) => {
            to_dto(&role).ok_or_else(|| AppError::NotFound(format!("template '{name}' not found")))
        }
        None => Err(AppError::NotFound(format!("template '{name}' not found"))),
    }
}

pub async fn create(client: &Client, input: TemplateInput) -> AppResult<TemplateDto> {
    validate(&input)?;
    let api: Api<ClusterRole> = Api::all(client.clone());
    let created = api
        .create(&PostParams::default(), &build(&input, false))
        .await
        .map_err(|e| match e {
            kube::Error::Api(s) if s.code == 409 => {
                AppError::Conflict(format!("template '{}' already exists", input.name))
            }
            e => e.into(),
        })?;
    to_dto(&created).ok_or_else(|| AppError::internal("created template is missing labels"))
}

pub async fn update(
    client: &Client,
    name: &str,
    mut input: TemplateInput,
) -> AppResult<TemplateDto> {
    input.name = name.to_string();
    validate(&input)?;
    let existing = get(client, name).await?;
    if existing.scope != input.scope {
        return Err(AppError::BadRequest(
            "the scope of a template cannot be changed".into(),
        ));
    }
    let api: Api<ClusterRole> = Api::all(client.clone());
    let applied = api
        .patch(
            &template_cluster_role(name),
            &PatchParams::apply(MANAGER).force(),
            &Patch::Apply(build(&input, existing.builtin)),
        )
        .await?;
    to_dto(&applied).ok_or_else(|| AppError::internal("updated template is missing labels"))
}

pub async fn delete(client: &Client, name: &str) -> AppResult<()> {
    get(client, name).await?;
    let in_use = format!("{LABEL_MANAGED_BY}={MANAGER},{LABEL_TEMPLATE_REF}={name}");
    let rbs: Api<RoleBinding> = Api::all(client.clone());
    let crbs: Api<ClusterRoleBinding> = Api::all(client.clone());
    let used = !rbs
        .list_metadata(&ListParams::default().labels(&in_use).limit(1))
        .await?
        .items
        .is_empty()
        || !crbs
            .list_metadata(&ListParams::default().labels(&in_use).limit(1))
            .await?
            .items
            .is_empty();
    if used {
        return Err(AppError::Conflict(format!(
            "template '{name}' is still bound to users; remove those bindings first"
        )));
    }
    // Also refuse when a group or user spec still references it, even if no
    // binding exists right now (e.g. a group without members).
    let groups: Api<IcecubeGroup> = Api::all(client.clone());
    if let Some(g) = groups
        .list(&ListParams::default())
        .await?
        .items
        .iter()
        .find(|g| g.spec.bindings.iter().any(|b| b.template == name))
    {
        return Err(AppError::Conflict(format!(
            "template '{name}' is used by group '{}'",
            g.metadata.name.as_deref().unwrap_or_default()
        )));
    }
    let users: Api<IcecubeUser> = Api::all(client.clone());
    if let Some(u) = users
        .list(&ListParams::default())
        .await?
        .items
        .iter()
        .find(|u| u.spec.bindings.iter().flatten().any(|b| b.template == name))
    {
        return Err(AppError::Conflict(format!(
            "template '{name}' is granted to user '{}'",
            u.metadata.name.as_deref().unwrap_or_default()
        )));
    }
    let api: Api<ClusterRole> = Api::all(client.clone());
    match api
        .delete(&template_cluster_role(name), &DeleteParams::default())
        .await
    {
        Ok(_) => Ok(()),
        Err(e) if is_not_found(&e) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn rule(groups: &[&str], resources: &[&str], verbs: &[&str]) -> PolicyRule {
    let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    PolicyRule {
        api_groups: Some(v(groups)),
        resources: Some(v(resources)),
        verbs: v(verbs),
        ..Default::default()
    }
}

const READ: &[&str] = &["get", "list", "watch"];
const WRITE: &[&str] = &[
    "get", "list", "watch", "create", "update", "patch", "delete",
];

fn builtins() -> Vec<TemplateInput> {
    vec![
        TemplateInput {
            name: "viewer".into(),
            scope: Scope::Namespaced,
            description: "Read-only access to workloads, services and config (no secrets).".into(),
            rules: vec![
                rule(
                    &[""],
                    &[
                        "pods",
                        "pods/log",
                        "services",
                        "endpoints",
                        "configmaps",
                        "persistentvolumeclaims",
                        "events",
                        "serviceaccounts",
                    ],
                    READ,
                ),
                rule(
                    &["apps"],
                    &["deployments", "statefulsets", "daemonsets", "replicasets"],
                    READ,
                ),
                rule(&["batch"], &["jobs", "cronjobs"], READ),
                rule(
                    &["networking.k8s.io"],
                    &["ingresses", "networkpolicies"],
                    READ,
                ),
                rule(&["autoscaling"], &["horizontalpodautoscalers"], READ),
            ],
        },
        TemplateInput {
            name: "developer".into(),
            scope: Scope::Namespaced,
            description: "Deploy and debug applications: manage workloads, exec and port-forward."
                .into(),
            rules: vec![
                rule(
                    &[""],
                    &[
                        "pods",
                        "services",
                        "configmaps",
                        "secrets",
                        "persistentvolumeclaims",
                        "serviceaccounts",
                    ],
                    WRITE,
                ),
                rule(&[""], &["pods/log", "events", "endpoints"], READ),
                rule(
                    &[""],
                    &["pods/exec", "pods/portforward", "pods/attach"],
                    &["create", "get"],
                ),
                rule(
                    &["apps"],
                    &["deployments", "statefulsets", "daemonsets", "replicasets"],
                    WRITE,
                ),
                rule(
                    &["apps"],
                    &["deployments/scale", "statefulsets/scale"],
                    &["get", "update", "patch"],
                ),
                rule(&["batch"], &["jobs", "cronjobs"], WRITE),
                rule(&["networking.k8s.io"], &["ingresses"], WRITE),
                rule(&["autoscaling"], &["horizontalpodautoscalers"], WRITE),
            ],
        },
        TemplateInput {
            name: "namespace-lister".into(),
            scope: Scope::Cluster,
            description: "List namespaces so tools like k9s and Lens can discover them.".into(),
            rules: vec![rule(&[""], &["namespaces"], READ)],
        },
        TemplateInput {
            name: "cluster-viewer".into(),
            scope: Scope::Cluster,
            description: "Read-only access to cluster resources: nodes, storage, CRDs.".into(),
            rules: vec![
                rule(&[""], &["nodes", "namespaces", "persistentvolumes"], READ),
                rule(&["storage.k8s.io"], &["storageclasses"], READ),
                rule(
                    &["apiextensions.k8s.io"],
                    &["customresourcedefinitions"],
                    READ,
                ),
                rule(&["metrics.k8s.io"], &["nodes", "pods"], READ),
            ],
        },
    ]
}

/// Create the builtin templates when missing. Existing ones are left alone so
/// edits made from the UI survive restarts.
pub async fn seed_builtins(client: &Client) -> anyhow::Result<()> {
    let api: Api<ClusterRole> = Api::all(client.clone());
    for input in builtins() {
        match api
            .create(&PostParams::default(), &build(&input, true))
            .await
        {
            Ok(_) => tracing::info!(template = %input.name, "seeded builtin template"),
            Err(kube::Error::Api(s)) if s.code == 409 => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
