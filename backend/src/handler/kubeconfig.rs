use axum::{
    Json,
    extract::{Path, State},
};

use crate::{
    library::{
        error::AppResult,
        model::{KubeconfigDto, KubeconfigInput},
        state::AppState,
    },
    usecase::kubeconfig,
};

pub async fn get(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<KubeconfigDto>> {
    Ok(Json(kubeconfig::stored(&s, &name).await?))
}

pub async fn issue(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<KubeconfigInput>,
) -> AppResult<Json<KubeconfigDto>> {
    Ok(Json(kubeconfig::issue(&s, &name, input).await?))
}

pub async fn reissue(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<KubeconfigDto>> {
    Ok(Json(kubeconfig::reissue(&s, &name).await?))
}
