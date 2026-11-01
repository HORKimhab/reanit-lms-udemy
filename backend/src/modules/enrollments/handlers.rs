use axum::{Json, Router, extract::State, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Enrollment;
use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::ApiJson,
    modules::{
        catalog::{CourseCard, to_cards},
        courses::model::CourseStatus,
    },
    state::AppState,
};

/// Mounted under `/me`.
pub fn router() -> Router<AppState> {
    Router::new().route("/courses", get(my_courses).post(enroll_free))
}

#[derive(Deserialize)]
struct EnrollInput {
    course_id: Uuid,
}

/// Joins a free, published course.
async fn enroll_free(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(input): ApiJson<EnrollInput>,
) -> AppResult<Json<Enrollment>> {
    let course = state
        .courses
        .find(input.course_id)
        .await?
        .filter(|c| c.status == CourseStatus::Published)
        .ok_or(AppError::NotFound)?;
    if course.price_minor > 0 {
        return Err(AppError::BadRequest(
            "this course must be purchased before you can join it".into(),
        ));
    }
    let enrollment = Enrollment {
        user_id: user.id,
        course_id: course.id,
        order_id: None,
        enrolled_at: state.clock.now(),
    };
    state.enrollments.enroll(&enrollment).await?;
    Ok(Json(enrollment))
}

#[derive(Serialize)]
struct MyCourse {
    course: CourseCard,
    enrolled_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct MyCourses {
    data: Vec<MyCourse>,
}

async fn my_courses(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<MyCourses>> {
    let enrollments = state.enrollments.list_by_user(user.id).await?;
    let ids: Vec<Uuid> = enrollments.iter().map(|e| e.course_id).collect();
    let mut cards = to_cards(&state, state.courses.find_many(&ids).await?).await?;
    let data = enrollments
        .into_iter()
        .filter_map(|e| {
            let pos = cards.iter().position(|c| c.id == e.course_id)?;
            Some(MyCourse {
                course: cards.swap_remove(pos),
                enrolled_at: e.enrolled_at,
            })
        })
        .collect();
    Ok(Json(MyCourses { data }))
}
