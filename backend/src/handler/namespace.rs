use axum::{Json, extract::State};

use crate::{
    library::{error::AppResult, state::AppState},
    usecase::user,
};

pub async fn list(State(s): State<AppState>) -> AppResult<Json<Vec<String>>> {
    Ok(Json(user::list_namespaces(&s.client).await?))
}
