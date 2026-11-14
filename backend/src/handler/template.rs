use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    library::{
        error::AppResult,
        model::{TemplateDto, TemplateInput},
        state::AppState,
    },
    usecase::template,
};

pub async fn list(State(s): State<AppState>) -> AppResult<Json<Vec<TemplateDto>>> {
    Ok(Json(template::list(&s.client).await?))
}

pub async fn get(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<TemplateDto>> {
    Ok(Json(template::get(&s.client, &name).await?))
}

pub async fn create(
    State(s): State<AppState>,
    Json(input): Json<TemplateInput>,
) -> AppResult<(StatusCode, Json<TemplateDto>)> {
    Ok((
        StatusCode::CREATED,
        Json(template::create(&s.client, input).await?),
    ))
}

pub async fn update(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<TemplateInput>,
) -> AppResult<Json<TemplateDto>> {
    Ok(Json(template::update(&s.client, &name, input).await?))
}

pub async fn delete(State(s): State<AppState>, Path(name): Path<String>) -> AppResult<StatusCode> {
    template::delete(&s.client, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}
