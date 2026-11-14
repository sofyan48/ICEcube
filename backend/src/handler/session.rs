use axum::{Json, extract::State};
use serde::Serialize;

use crate::library::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    user: String,
    cluster_name: String,
    server: String,
    max_cert_days: u32,
    stores_kubeconfigs: bool,
}

pub async fn get(State(s): State<AppState>) -> Json<Session> {
    Json(Session {
        user: s.config.auth_user.clone(),
        cluster_name: s.config.cluster_name.clone(),
        server: s.server_url.clone(),
        max_cert_days: s.config.cert_days,
        stores_kubeconfigs: s.config.store_kubeconfigs,
    })
}
