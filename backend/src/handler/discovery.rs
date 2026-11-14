use axum::{Json, extract::State};

use crate::{
    library::{error::AppResult, model::ApiGroupDto, state::AppState},
    usecase::discovery,
};

pub async fn list(State(s): State<AppState>) -> AppResult<Json<Vec<ApiGroupDto>>> {
    Ok(Json(discovery::list(&s.client).await?))
}
