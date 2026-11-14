use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde_json::json;
use subtle::ConstantTimeEq;

use crate::library::state::AppState;

/// HTTP Basic auth on every API call. The UI keeps the credentials in memory
/// and sends them itself; no `WWW-Authenticate` header is returned so the
/// browser does not show its own prompt.
pub async fn basic_auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| B64.decode(v).ok())
        .and_then(|v| String::from_utf8(v).ok())
        .is_some_and(|creds| {
            let expected = format!("{}:{}", state.config.auth_user, state.config.auth_password);
            creds.as_bytes().ct_eq(expected.as_bytes()).into()
        });
    if authorized {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid credentials" })),
        )
            .into_response()
    }
}
