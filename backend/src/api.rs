use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde::Serialize;
use serde_json::json;
use subtle::ConstantTimeEq;

use crate::{error::AppResult, groups, kubeconfig, model::*, state::AppState, templates, users};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/session", get(session))
        .route("/namespaces", get(namespaces))
        .route("/templates", get(list_templates).post(create_template))
        .route(
            "/templates/{name}",
            get(get_template)
                .put(update_template)
                .delete(delete_template),
        )
        .route("/groups", get(list_groups).post(create_group))
        .route(
            "/groups/{name}",
            axum::routing::put(update_group).delete(delete_group),
        )
        .route("/users", get(list_users).post(create_user))
        .route(
            "/users/{name}",
            get(get_user).put(update_user).delete(delete_user),
        )
        .route("/users/{name}/kubeconfig", post(issue_kubeconfig))
        .route("/users/{name}/kubeconfig/reissue", post(reissue_kubeconfig))
        .layer(middleware::from_fn_with_state(state.clone(), basic_auth))
        .with_state(state)
}

/// HTTP Basic auth on every API call. The UI keeps the credentials in memory
/// and sends them itself; no `WWW-Authenticate` header is returned so the
/// browser does not show its own prompt.
async fn basic_auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    user: String,
    cluster_name: String,
    server: String,
    max_cert_days: u32,
}

async fn session(State(s): State<AppState>) -> Json<Session> {
    Json(Session {
        user: s.config.auth_user.clone(),
        cluster_name: s.config.cluster_name.clone(),
        server: s.server_url.clone(),
        max_cert_days: s.config.cert_days,
    })
}

async fn namespaces(State(s): State<AppState>) -> AppResult<Json<Vec<String>>> {
    Ok(Json(users::list_namespaces(&s.client).await?))
}

async fn list_templates(State(s): State<AppState>) -> AppResult<Json<Vec<TemplateDto>>> {
    Ok(Json(templates::list(&s.client).await?))
}

async fn get_template(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<TemplateDto>> {
    Ok(Json(templates::get(&s.client, &name).await?))
}

async fn create_template(
    State(s): State<AppState>,
    Json(input): Json<TemplateInput>,
) -> AppResult<(StatusCode, Json<TemplateDto>)> {
    Ok((
        StatusCode::CREATED,
        Json(templates::create(&s.client, input).await?),
    ))
}

async fn update_template(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<TemplateInput>,
) -> AppResult<Json<TemplateDto>> {
    Ok(Json(templates::update(&s.client, &name, input).await?))
}

async fn delete_template(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<StatusCode> {
    templates::delete(&s.client, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_users(State(s): State<AppState>) -> AppResult<Json<Vec<UserDto>>> {
    Ok(Json(users::list(&s.client).await?))
}

async fn get_user(State(s): State<AppState>, Path(name): Path<String>) -> AppResult<Json<UserDto>> {
    Ok(Json(users::get(&s.client, &name).await?))
}

async fn create_user(
    State(s): State<AppState>,
    Json(input): Json<CreateUserInput>,
) -> AppResult<(StatusCode, Json<UserDto>)> {
    Ok((
        StatusCode::CREATED,
        Json(users::create(&s.client, input).await?),
    ))
}

async fn update_user(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<UpdateUserInput>,
) -> AppResult<Json<UserDto>> {
    Ok(Json(users::update(&s.client, &name, input).await?))
}

async fn list_groups(State(s): State<AppState>) -> AppResult<Json<Vec<GroupDto>>> {
    Ok(Json(groups::list(&s.client).await?))
}

async fn create_group(
    State(s): State<AppState>,
    Json(input): Json<GroupInput>,
) -> AppResult<(StatusCode, Json<GroupDto>)> {
    Ok((
        StatusCode::CREATED,
        Json(groups::create(&s.client, input).await?),
    ))
}

async fn update_group(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<GroupInput>,
) -> AppResult<Json<GroupDto>> {
    Ok(Json(groups::update(&s.client, &name, input).await?))
}

async fn delete_group(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<StatusCode> {
    groups::delete(&s.client, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user(State(s): State<AppState>, Path(name): Path<String>) -> AppResult<StatusCode> {
    users::delete(&s.client, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn issue_kubeconfig(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<KubeconfigInput>,
) -> AppResult<Json<KubeconfigDto>> {
    Ok(Json(kubeconfig::issue(&s, &name, input).await?))
}

async fn reissue_kubeconfig(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<KubeconfigDto>> {
    Ok(Json(kubeconfig::reissue(&s, &name).await?))
}
