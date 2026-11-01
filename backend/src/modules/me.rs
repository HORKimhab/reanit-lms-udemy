use axum::{Json, Router, extract::State, routing::post};

use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    modules::users::model::{Role, UserDto},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/become-instructor", post(become_instructor))
}

/// Students with a verified email can start teaching. Instructors and admins
/// are returned unchanged.
async fn become_instructor(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<Json<UserDto>> {
    let user = current.require_verified_email()?;
    if user.role != Role::Student {
        return Ok(Json(user.into()));
    }
    let updated = state
        .users
        .set_role(user.id, Role::Instructor)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json((&updated).into()))
}
