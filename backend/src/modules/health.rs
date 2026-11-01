use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};

use crate::{error::AppResult, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health(State(state): State<AppState>) -> AppResult<Json<Value>> {
    state.health.check().await?;
    Ok(Json(json!({ "status": "ok" })))
}
