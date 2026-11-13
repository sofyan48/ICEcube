//! Keeps each user's latest kubeconfig in a Secret, so it can be viewed and
//! downloaded again after a page reload or from another browser.
//!
//! One Secret per user (`icecube-kubeconfig-<user>`) in ICEcube's namespace,
//! overwritten on every issue and owned by the IcecubeUser, so deleting the
//! user deletes it too. Reading Secrets in that namespace already exposes the
//! admin password (`icecube-auth`), which can mint any kubeconfig, so storing
//! them there does not widen who can act as a user.

use std::collections::BTreeMap;

use k8s_openapi::{ByteString, api::core::v1::Secret};
use kube::{
    Api, Resource,
    api::{DeleteParams, ObjectMeta, Patch, PatchParams},
};

use crate::{
    error::{AppError, AppResult, is_not_found},
    model::*,
    state::AppState,
};

const KEY_KUBECONFIG: &str = "kubeconfig";
const KEY_CERTIFICATE: &str = "certificate.json";

fn secret_name(user: &str) -> String {
    format!("icecube-kubeconfig-{user}")
}

fn api(state: &AppState) -> Api<Secret> {
    Api::namespaced(state.client.clone(), &state.namespace)
}

/// Store (or replace) the user's kubeconfig. No-op when storage is disabled.
pub async fn save(state: &AppState, user: &IcecubeUser, issued: &KubeconfigDto) -> AppResult<()> {
    if !state.config.store_kubeconfigs {
        return Ok(());
    }
    let name = user.metadata.name.clone().unwrap_or_default();
    // Cluster-scoped owners may own namespaced objects: deleting the user
    // garbage-collects the Secret even if ICEcube is not running.
    let owner = user.controller_owner_ref(&()).map(|mut o| {
        o.controller = None;
        o.block_owner_deletion = None;
        o
    });
    let certificate = serde_json::to_vec(&issued.certificate).map_err(AppError::internal)?;
    let secret = Secret {
        metadata: ObjectMeta {
            name: Some(secret_name(&name)),
            namespace: Some(state.namespace.clone()),
            labels: Some(BTreeMap::from([
                (LABEL_MANAGED_BY.into(), MANAGER.into()),
                (LABEL_USER.into(), name.clone()),
            ])),
            owner_references: owner.map(|o| vec![o]),
            ..Default::default()
        },
        type_: Some("Opaque".into()),
        data: Some(BTreeMap::from([
            (
                KEY_KUBECONFIG.into(),
                ByteString(issued.kubeconfig.clone().into_bytes()),
            ),
            (KEY_CERTIFICATE.into(), ByteString(certificate)),
        ])),
        ..Default::default()
    };
    api(state)
        .patch(
            &secret_name(&name),
            &PatchParams::apply(MANAGER).force(),
            &Patch::Apply(&secret),
        )
        .await?;
    Ok(())
}

/// The user's stored kubeconfig, if any.
pub async fn load(state: &AppState, user: &str) -> AppResult<KubeconfigDto> {
    if !state.config.store_kubeconfigs {
        return Err(AppError::NotFound(
            "kubeconfig storage is disabled; issue a new one".into(),
        ));
    }
    let not_found = || AppError::NotFound(format!("no stored kubeconfig for user '{user}'"));
    let secret = api(state)
        .get_opt(&secret_name(user))
        .await?
        .ok_or_else(not_found)?;
    let data = secret.data.unwrap_or_default();
    let kubeconfig = data
        .get(KEY_KUBECONFIG)
        .and_then(|b| String::from_utf8(b.0.clone()).ok())
        .ok_or_else(not_found)?;
    let certificate = data
        .get(KEY_CERTIFICATE)
        .and_then(|b| serde_json::from_slice(&b.0).ok())
        .ok_or_else(not_found)?;
    Ok(KubeconfigDto {
        kubeconfig,
        certificate,
    })
}

/// Remove the user's stored kubeconfig. Missing is fine.
pub async fn delete(state: &AppState, user: &str) -> AppResult<()> {
    if !state.config.store_kubeconfigs {
        return Ok(());
    }
    match api(state)
        .delete(&secret_name(user), &DeleteParams::default())
        .await
    {
        Ok(_) => Ok(()),
        Err(e) if is_not_found(&e) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
