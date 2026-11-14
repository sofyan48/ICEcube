use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    library::{
        error::AppResult,
        model::{CreateUserInput, UpdateUserInput, UserDto},
        state::AppState,
    },
    usecase::user,
};

pub async fn list(State(s): State<AppState>) -> AppResult<Json<Vec<UserDto>>> {
    Ok(Json(user::list(&s.client).await?))
}

pub async fn get(State(s): State<AppState>, Path(name): Path<String>) -> AppResult<Json<UserDto>> {
    Ok(Json(user::get(&s.client, &name).await?))
}

pub async fn create(
    State(s): State<AppState>,
    Json(input): Json<CreateUserInput>,
) -> AppResult<(StatusCode, Json<UserDto>)> {
    Ok((
        StatusCode::CREATED,
        Json(user::create(&s.client, input).await?),
    ))
}

pub async fn update(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<UpdateUserInput>,
) -> AppResult<Json<UserDto>> {
    Ok(Json(user::update(&s.client, &name, input).await?))
}

pub async fn delete(State(s): State<AppState>, Path(name): Path<String>) -> AppResult<StatusCode> {
    user::delete(&s, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}
