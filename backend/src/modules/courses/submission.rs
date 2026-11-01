//! Submitting a course for admin review.

use axum::{Json, extract::State};
use uuid::Uuid;

use super::{
    handlers::{load_managed_course, require_instructor, to_dto},
    model::{Course, CourseDto, CourseStatus},
};
use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::ApiPath,
    modules::curriculum::{
        handlers::load_curriculum,
        model::{LectureKind, SectionWithLectures},
    },
    state::AppState,
};

/// Everything that must be fixed before the course can be reviewed.
pub fn submission_problems(course: &Course, curriculum: &[SectionWithLectures]) -> Vec<String> {
    let mut problems = Vec::new();
    if course.subtitle.is_empty() {
        problems.push("add a subtitle".to_string());
    }
    if course.description.is_empty() {
        problems.push("add a description".to_string());
    }
    if course.category_id.is_none() {
        problems.push("choose a category".to_string());
    }
    if course.thumbnail_key.is_none() {
        problems.push("upload a thumbnail".to_string());
    }
    if curriculum.iter().all(|s| s.lectures.is_empty()) {
        problems.push("add at least one lecture".to_string());
    }
    for s in curriculum {
        if s.lectures.is_empty() {
            problems.push(format!("section \"{}\" has no lectures", s.section.title));
        }
        for l in &s.lectures {
            match l.kind {
                LectureKind::Video if l.video_asset_id.is_none() => {
                    problems.push(format!("lecture \"{}\" has no video", l.title))
                }
                LectureKind::Article if l.article_body.trim().is_empty() => {
                    problems.push(format!("lecture \"{}\" has no content", l.title))
                }
                _ => {}
            }
        }
    }
    problems
}

pub async fn submit(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<CourseDto>> {
    let user = require_instructor(&current)?;
    let mut course = load_managed_course(&state, user, id).await?;
    if !matches!(course.status, CourseStatus::Draft | CourseStatus::Rejected) {
        return Err(AppError::Conflict(
            "only draft or rejected courses can be submitted".into(),
        ));
    }

    let curriculum = load_curriculum(&state, course.id).await?;
    let problems = submission_problems(&course, &curriculum);
    if !problems.is_empty() {
        return Err(AppError::BadRequest(format!(
            "course is not ready for review: {}",
            problems.join("; ")
        )));
    }

    course.status = CourseStatus::Pending;
    course.rejection_reason = None;
    course.updated_at = state.clock.now();
    state.courses.save(&course).await?;
    Ok(Json(to_dto(&state, course)))
}
