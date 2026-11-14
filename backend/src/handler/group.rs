use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    library::{
        error::AppResult,
        model::{GroupDto, GroupInput},
        state::AppState,
    },
    usecase::group,
};

pub async fn list(State(s): State<AppState>) -> AppResult<Json<Vec<GroupDto>>> {
    Ok(Json(group::list(&s.client).await?))
}

pub async fn create(
    State(s): State<AppState>,
    Json(input): Json<GroupInput>,
) -> AppResult<(StatusCode, Json<GroupDto>)> {
    Ok((
        StatusCode::CREATED,
        Json(group::create(&s.client, input).await?),
    ))
}

pub async fn update(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<GroupInput>,
) -> AppResult<Json<GroupDto>> {
    Ok(Json(group::update(&s.client, &name, input).await?))
}

pub async fn delete(State(s): State<AppState>, Path(name): Path<String>) -> AppResult<StatusCode> {
    group::delete(&s.client, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}
