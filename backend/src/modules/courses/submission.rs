//! Submitting a course for admin review.

use std::collections::HashMap;

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
    modules::{
        curriculum::{
            handlers::load_curriculum,
            model::{LectureKind, SectionWithLectures},
        },
        videos::model::VideoStatus,
    },
    state::AppState,
};

/// Everything that must be fixed before the course can be reviewed.
/// `videos` maps each attached video asset to its processing status.
pub fn submission_problems(
    course: &Course,
    curriculum: &[SectionWithLectures],
    videos: &HashMap<Uuid, VideoStatus>,
) -> Vec<String> {
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
            match (l.kind, l.video_asset_id.and_then(|id| videos.get(&id))) {
                (LectureKind::Video, None) => {
                    problems.push(format!("lecture \"{}\" has no video", l.title))
                }
                (LectureKind::Video, Some(VideoStatus::Uploading | VideoStatus::Processing)) => {
                    problems.push(format!("lecture \"{}\" is still processing", l.title))
                }
                (LectureKind::Video, Some(VideoStatus::Failed)) => problems.push(format!(
                    "the video for lecture \"{}\" failed to process; upload it again",
                    l.title
                )),
                (LectureKind::Article, _) if l.article_body.trim().is_empty() => {
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
    let mut videos = HashMap::new();
    for id in curriculum
        .iter()
        .flat_map(|s| &s.lectures)
        .filter_map(|l| l.video_asset_id)
    {
        if let Some(asset) = state.video_assets.find(id).await? {
            videos.insert(id, asset.status);
        }
    }
    let problems = submission_problems(&course, &curriculum, &videos);
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
