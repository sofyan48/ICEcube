//! Usernames that were deleted while client certificates issued to them were
//! still valid. Kubernetes cannot revoke a certificate, so binding the same
//! name again would silently bring those certificates back to life. The name
//! stays blocked until the last of them expires.
//!
//! Kept in one ConfigMap in ICEcube's namespace: key = username, value = RFC
//! 3339 time until which the name is blocked. Written with JSON merge patches
//! on single keys, so concurrent writers never overwrite each other.

use std::collections::BTreeMap;

use jiff::Timestamp;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{
    Api,
    api::{ObjectMeta, Patch, PatchParams, PostParams},
};
use serde_json::json;

use crate::library::{
    error::{AppError, AppResult},
    model::*,
    state::AppState,
};

const CONFIG_MAP: &str = "icecube-deleted-users";

fn api(state: &AppState) -> Api<ConfigMap> {
    Api::namespaced(state.client.clone(), &state.namespace)
}

async fn entries(state: &AppState) -> AppResult<BTreeMap<String, String>> {
    Ok(api(state)
        .get_opt(CONFIG_MAP)
        .await?
        .and_then(|cm| cm.data)
        .unwrap_or_default())
}

/// Until when `user` is blocked, if it still is.
pub async fn active(state: &AppState, user: &str) -> AppResult<Option<Timestamp>> {
    Ok(entries(state)
        .await?
        .get(user)
        .and_then(|v| v.parse::<Timestamp>().ok())
        .filter(|until| *until > Timestamp::now()))
}

/// Block `user` until `until` (keeps a later existing entry). Expired entries
/// of other users are pruned on the way.
pub async fn record(state: &AppState, user: &str, until: Timestamp) -> AppResult<()> {
    let now = Timestamp::now();
    let existing = entries(state).await?;
    let until = existing
        .get(user)
        .and_then(|v| v.parse::<Timestamp>().ok())
        .map_or(until, |t| t.max(until));

    let mut data = serde_json::Map::new();
    for (name, value) in &existing {
        if value.parse::<Timestamp>().map_or(true, |t| t <= now) {
            data.insert(name.clone(), serde_json::Value::Null);
        }
    }
    data.insert(user.to_string(), json!(until.to_string()));
    patch(state, data).await?;
    tracing::info!(user, %until, "username blocked until its certificates expire");
    Ok(())
}

/// Allow `user` again. Missing is fine.
pub async fn clear(state: &AppState, user: &str) -> AppResult<()> {
    if !entries(state).await?.contains_key(user) {
        return Ok(());
    }
    let mut data = serde_json::Map::new();
    data.insert(user.to_string(), serde_json::Value::Null);
    patch(state, data).await
}

async fn patch(
    state: &AppState,
    data: serde_json::Map<String, serde_json::Value>,
) -> AppResult<()> {
    let api = api(state);
    let body = json!({ "data": data });
    match api
        .patch(CONFIG_MAP, &PatchParams::default(), &Patch::Merge(&body))
        .await
    {
        Ok(_) => return Ok(()),
        Err(kube::Error::Api(s)) if s.code == 404 => {}
        Err(e) => return Err(e.into()),
    }
    let cm = ConfigMap {
        metadata: ObjectMeta {
            name: Some(CONFIG_MAP.into()),
            namespace: Some(state.namespace.clone()),
            labels: Some(BTreeMap::from([(LABEL_MANAGED_BY.into(), MANAGER.into())])),
            ..Default::default()
        },
        data: Some(
            data.into_iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k, v.to_string())))
                .collect(),
        ),
        ..Default::default()
    };
    match api.create(&PostParams::default(), &cm).await {
        Ok(_) => Ok(()),
        // Someone else created it meanwhile; patch into theirs.
        Err(kube::Error::Api(s)) if s.code == 409 => api
            .patch(CONFIG_MAP, &PatchParams::default(), &Patch::Merge(&body))
            .await
            .map(|_| ())
            .map_err(AppError::from),
        Err(e) => Err(e.into()),
    }
}
