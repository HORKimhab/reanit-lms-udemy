use async_trait::async_trait;
use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;
use uuid::Uuid;

use crate::{error::AppResult, state::AppState};

#[derive(Clone, Debug, Serialize)]
pub struct Category {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
}

#[async_trait]
pub trait CategoryRepo: Send + Sync {
    /// All categories in display order.
    async fn list(&self) -> AppResult<Vec<Category>>;
    async fn find(&self, id: Uuid) -> AppResult<Option<Category>>;
}

pub fn router() -> Router<AppState> {
    Router::new().route("/categories", get(list))
}

async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Category>>> {
    Ok(Json(state.categories.list().await?))
}
