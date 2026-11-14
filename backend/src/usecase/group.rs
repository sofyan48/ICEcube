//! Groups (IcecubeGroup CRs): named sets of bindings that users join.
//! The controller watches groups and re-applies the bindings of every member.

use std::collections::BTreeMap;

use kube::{
    Api, Client, ResourceExt,
    api::{DeleteParams, ListParams, Patch, PatchParams, PostParams},
};

use crate::{
    library::{
        error::{AppError, AppResult, is_not_found},
        model::*,
    },
    usecase::user as users,
};

pub const DEFAULT_GROUP: &str = "default";

fn to_dto(g: &IcecubeGroup) -> GroupDto {
    let mut bindings = g.spec.bindings.clone();
    bindings.sort();
    GroupDto {
        name: g.name_any(),
        description: g.spec.description.clone(),
        bindings,
        default_for_new_users: g.spec.default_for_new_users,
    }
}

pub async fn list_crs(client: &Client) -> AppResult<Vec<IcecubeGroup>> {
    let api: Api<IcecubeGroup> = Api::all(client.clone());
    Ok(api.list(&ListParams::default()).await?.items)
}

pub async fn list(client: &Client) -> AppResult<Vec<GroupDto>> {
    let mut out: Vec<_> = list_crs(client).await?.iter().map(to_dto).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

async fn get_cr(client: &Client, name: &str) -> AppResult<IcecubeGroup> {
    let api: Api<IcecubeGroup> = Api::all(client.clone());
    api.get_opt(name)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("group '{name}' not found")))
}

fn description(input: &GroupInput) -> Option<String> {
    input.description.clone().filter(|d| !d.trim().is_empty())
}

pub async fn create(client: &Client, input: GroupInput) -> AppResult<GroupDto> {
    validate_name("group", &input.name)?;
    let bindings = users::validate_bindings(client, &input.bindings).await?;
    let mut cr = IcecubeGroup::new(
        &input.name,
        IcecubeGroupSpec {
            description: description(&input),
            bindings: bindings.into_iter().collect(),
            default_for_new_users: input.default_for_new_users,
        },
    );
    cr.metadata.labels = Some(BTreeMap::from([(LABEL_MANAGED_BY.into(), MANAGER.into())]));
    let api: Api<IcecubeGroup> = Api::all(client.clone());
    let created = api
        .create(&PostParams::default(), &cr)
        .await
        .map_err(|e| match e {
            kube::Error::Api(s) if s.code == 409 => {
                AppError::Conflict(format!("group '{}' already exists", input.name))
            }
            e => e.into(),
        })?;
    // A new group has no members yet, so there is nothing to reconcile.
    Ok(to_dto(&created))
}

pub async fn update(client: &Client, name: &str, input: GroupInput) -> AppResult<GroupDto> {
    get_cr(client, name).await?;
    let bindings = users::validate_bindings(client, &input.bindings).await?;
    // Explicit JSON so a cleared description is sent as null and removed.
    let patch = serde_json::json!({
        "spec": {
            "description": description(&input),
            "bindings": bindings,
            "defaultForNewUsers": input.default_for_new_users,
        }
    });
    let api: Api<IcecubeGroup> = Api::all(client.clone());
    let updated = api
        .patch(name, &PatchParams::default(), &Patch::Merge(patch))
        .await?;
    Ok(to_dto(&updated))
}

/// Deleting a group removes it from every member and revokes what it granted.
pub async fn delete(client: &Client, name: &str) -> AppResult<()> {
    get_cr(client, name).await?;
    let user_api: Api<IcecubeUser> = Api::all(client.clone());
    for user in users::list_crs(client).await? {
        if !user.spec.groups.iter().any(|g| g == name) {
            continue;
        }
        let groups: Vec<_> = user.spec.groups.iter().filter(|g| *g != name).collect();
        user_api
            .patch(
                &user.name_any(),
                &PatchParams::default(),
                &Patch::Merge(serde_json::json!({ "spec": { "groups": groups } })),
            )
            .await?;
    }

    let api: Api<IcecubeGroup> = Api::all(client.clone());
    match api.delete(name, &DeleteParams::default()).await {
        Ok(_) => Ok(()),
        Err(e) if is_not_found(&e) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// On first install, create the "default" group that every new user joins.
/// Only seeded when no group exists at all, so deleting or renaming it sticks
/// as long as at least one group remains.
pub async fn seed_default(client: &Client) -> anyhow::Result<()> {
    if !list_crs(client).await?.is_empty() {
        return Ok(());
    }
    let binding = |template: &str| BindingDto {
        template: template.into(),
        namespace: None,
    };
    let mut cr = IcecubeGroup::new(
        DEFAULT_GROUP,
        IcecubeGroupSpec {
            description: Some(
                "Baseline access for every new user: discover namespaces, see cluster resources and read workloads."
                    .into(),
            ),
            bindings: vec![binding("cluster-viewer"), binding("namespace-lister"), binding("viewer")],
            default_for_new_users: true,
        },
    );
    cr.metadata.labels = Some(BTreeMap::from([(LABEL_MANAGED_BY.into(), MANAGER.into())]));
    let api: Api<IcecubeGroup> = Api::all(client.clone());
    match api.create(&PostParams::default(), &cr).await {
        Ok(_) => tracing::info!(group = DEFAULT_GROUP, "seeded default group"),
        Err(kube::Error::Api(s)) if s.code == 409 => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
