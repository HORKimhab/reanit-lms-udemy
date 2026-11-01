use axum::{Json, Router, extract::State, routing::patch};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::{ApiJson, ApiPath},
    modules::users::model::{Role, UserDto},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/users/{id}/role", patch(set_role))
}

#[derive(Deserialize)]
struct SetRoleRequest {
    role: Role,
}

async fn set_role(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<SetRoleRequest>,
) -> AppResult<Json<UserDto>> {
    current.require_role(Role::Admin)?;
    let user = state
        .users
        .set_role(id, req.role)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json((&user).into()))
}
