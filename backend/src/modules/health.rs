use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};

use crate::{error::AppResult, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health(State(state): State<AppState>) -> AppResult<Json<Value>> {
    sqlx::query("SELECT 1").execute(&state.db).await?;
    let mut redis = state.redis.clone();
    let _: String = redis::cmd("PING").query_async(&mut redis).await?;

    Ok(Json(json!({ "status": "ok", "db": "ok", "redis": "ok" })))
}
