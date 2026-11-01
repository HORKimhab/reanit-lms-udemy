use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{get, patch, post},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::{ApiJson, ApiPath},
    modules::{
        courses::{
            handlers::to_dto,
            model::{CourseDto, CourseStatus},
        },
        users::model::{Role, UserDto},
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users/{id}/role", patch(set_role))
        .route("/courses", get(list_courses))
        .route("/courses/{id}/approve", post(approve))
        .route("/courses/{id}/reject", post(reject))
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

#[derive(Deserialize)]
struct ListQuery {
    status: CourseStatus,
}

#[derive(Serialize)]
struct AdminCourse {
    #[serde(flatten)]
    course: CourseDto,
    instructor_name: String,
}

async fn list_courses(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<AdminCourse>>> {
    current.require_role(Role::Admin)?;
    let mut out = Vec::new();
    for course in state.courses.list_by_status(q.status).await? {
        let instructor_name = state
            .users
            .find_by_id(course.instructor_id)
            .await?
            .map(|u| u.name)
            .unwrap_or_default();
        out.push(AdminCourse {
            course: to_dto(&state, course),
            instructor_name,
        });
    }
    Ok(Json(out))
}

async fn pending_course(
    state: &AppState,
    current: &CurrentUser,
    id: Uuid,
) -> AppResult<crate::modules::courses::model::Course> {
    current.require_role(Role::Admin)?;
    let course = state.courses.find(id).await?.ok_or(AppError::NotFound)?;
    if course.status != CourseStatus::Pending {
        return Err(AppError::Conflict(
            "only courses waiting for review can be reviewed".into(),
        ));
    }
    Ok(course)
}

async fn approve(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<CourseDto>> {
    let mut course = pending_course(&state, &current, id).await?;
    let now = state.clock.now();
    course.status = CourseStatus::Published;
    course.rejection_reason = None;
    course.published_at.get_or_insert(now);
    course.updated_at = now;
    state.courses.save(&course).await?;
    Ok(Json(to_dto(&state, course)))
}

#[derive(Deserialize)]
struct RejectRequest {
    reason: String,
}

async fn reject(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<RejectRequest>,
) -> AppResult<Json<CourseDto>> {
    let mut course = pending_course(&state, &current, id).await?;
    let reason = req.reason.trim();
    if reason.is_empty() || reason.chars().count() > 2000 {
        return Err(AppError::BadRequest(
            "reason must be 1-2000 characters".into(),
        ));
    }
    course.status = CourseStatus::Rejected;
    course.rejection_reason = Some(reason.to_string());
    course.updated_at = state.clock.now();
    state.courses.save(&course).await?;
    Ok(Json(to_dto(&state, course)))
}
