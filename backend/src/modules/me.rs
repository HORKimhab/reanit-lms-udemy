use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};

use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::ApiJson,
    modules::{
        instructors::{InstructorProfile, ProfileInput},
        users::model::{Role, UserDto},
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/become-instructor", post(become_instructor))
        .route("/instructor-profile", get(get_profile).put(update_profile))
        .merge(crate::modules::enrollments::handlers::router())
}

/// Students with a verified email can start teaching by filling in a profile.
/// Instructors and admins keep their role (and update their profile).
async fn become_instructor(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(input): ApiJson<ProfileInput>,
) -> AppResult<Json<UserDto>> {
    let user = current.require_verified_email()?;
    let profile = input.validate(user.id)?;
    state.instructor_profiles.upsert(profile).await?;

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

async fn get_profile(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<InstructorProfile>> {
    let profile = state
        .instructor_profiles
        .find(user.id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(profile))
}

async fn update_profile(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(input): ApiJson<ProfileInput>,
) -> AppResult<Json<InstructorProfile>> {
    if state.instructor_profiles.find(user.id).await?.is_none() {
        return Err(AppError::NotFound);
    }
    let profile = input.validate(user.id)?;
    Ok(Json(state.instructor_profiles.upsert(profile).await?))
}
