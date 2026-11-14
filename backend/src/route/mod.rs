//! HTTP routes. Only maps paths to handlers; the work happens in `handler`.

use axum::{
    Router, middleware,
    routing::{get, post, put},
};

use crate::{
    handler::{auth, group, kubeconfig, namespace, session, template, ui, user},
    library::state::AppState,
};

/// The whole application: health check, the API under `/api` and the UI as fallback.
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api", api(state))
        .fallback(ui::serve)
}

pub fn api(state: AppState) -> Router {
    Router::new()
        .route("/session", get(session::get))
        .route("/namespaces", get(namespace::list))
        .route("/templates", get(template::list).post(template::create))
        .route(
            "/templates/{name}",
            get(template::get)
                .put(template::update)
                .delete(template::delete),
        )
        .route("/groups", get(group::list).post(group::create))
        .route("/groups/{name}", put(group::update).delete(group::delete))
        .route("/users", get(user::list).post(user::create))
        .route(
            "/users/{name}",
            get(user::get).put(user::update).delete(user::delete),
        )
        .route(
            "/users/{name}/kubeconfig",
            get(kubeconfig::get).post(kubeconfig::issue),
        )
        .route(
            "/users/{name}/kubeconfig/reissue",
            post(kubeconfig::reissue),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::basic_auth,
        ))
        .with_state(state)
}
