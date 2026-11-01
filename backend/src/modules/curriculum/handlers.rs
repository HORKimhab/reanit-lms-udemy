use std::collections::HashSet;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, patch, post, put},
};
use serde::Deserialize;
use uuid::Uuid;

use super::{
    model::{Lecture, LectureKind, Section, SectionWithLectures, assemble},
    repo::SectionOrder,
};
use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::{ApiJson, ApiPath},
    modules::courses::{
        handlers::{load_editable_course, load_managed_course, require_instructor},
        model::Course,
    },
    state::AppState,
};

const MAX_TITLE: usize = 80;
const MAX_ARTICLE: usize = 100_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/courses/{course_id}/curriculum", get(get_curriculum))
        .route("/courses/{course_id}/curriculum/order", put(reorder))
        .route("/courses/{course_id}/sections", post(create_section))
        .route(
            "/courses/{course_id}/sections/{section_id}",
            patch(update_section).delete(delete_section),
        )
        .route(
            "/courses/{course_id}/sections/{section_id}/lectures",
            post(create_lecture),
        )
        .route(
            "/courses/{course_id}/lectures/{lecture_id}",
            patch(update_lecture).delete(delete_lecture),
        )
}

fn validate_title(title: &str) -> AppResult<String> {
    let title = title.trim().to_string();
    if title.is_empty() || title.chars().count() > MAX_TITLE {
        return Err(AppError::BadRequest(format!(
            "title must be 1-{MAX_TITLE} characters"
        )));
    }
    Ok(title)
}

async fn editable_course(state: &AppState, current: &CurrentUser, id: Uuid) -> AppResult<Course> {
    load_editable_course(state, require_instructor(current)?, id).await
}

async fn section_of(state: &AppState, course: &Course, id: Uuid) -> AppResult<Section> {
    match state.curriculum.find_section(id).await? {
        Some(s) if s.course_id == course.id => Ok(s),
        _ => Err(AppError::NotFound),
    }
}

async fn lecture_of(state: &AppState, course: &Course, id: Uuid) -> AppResult<Lecture> {
    match state.curriculum.find_lecture(id).await? {
        Some(l) if l.course_id == course.id => Ok(l),
        _ => Err(AppError::NotFound),
    }
}

pub async fn load_curriculum(
    state: &AppState,
    course_id: Uuid,
) -> AppResult<Vec<SectionWithLectures>> {
    let sections = state.curriculum.sections(course_id).await?;
    let lectures = state.curriculum.lectures(course_id).await?;
    Ok(assemble(sections, lectures))
}

async fn get_curriculum(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(course_id): ApiPath<Uuid>,
) -> AppResult<Json<Vec<SectionWithLectures>>> {
    let course = load_managed_course(&state, require_instructor(&current)?, course_id).await?;
    Ok(Json(load_curriculum(&state, course.id).await?))
}

#[derive(Deserialize)]
struct TitleBody {
    title: String,
}

async fn create_section(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(course_id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<TitleBody>,
) -> AppResult<(StatusCode, Json<Section>)> {
    let course = editable_course(&state, &current, course_id).await?;
    let position = next_position(
        state
            .curriculum
            .sections(course.id)
            .await?
            .iter()
            .map(|s| s.position),
    );
    let section = Section {
        id: Uuid::new_v4(),
        course_id: course.id,
        title: validate_title(&req.title)?,
        position,
    };
    state.curriculum.insert_section(&section).await?;
    Ok((StatusCode::CREATED, Json(section)))
}

fn next_position(positions: impl Iterator<Item = i32>) -> i32 {
    positions.max().map_or(0, |p| p + 1)
}

async fn update_section(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((course_id, section_id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<TitleBody>,
) -> AppResult<Json<Section>> {
    let course = editable_course(&state, &current, course_id).await?;
    let mut section = section_of(&state, &course, section_id).await?;
    section.title = validate_title(&req.title)?;
    state.curriculum.save_section(&section).await?;
    Ok(Json(section))
}

async fn delete_section(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((course_id, section_id)): ApiPath<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let course = editable_course(&state, &current, course_id).await?;
    let section = section_of(&state, &course, section_id).await?;
    state.curriculum.delete_section(section.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CreateLecture {
    title: String,
    kind: LectureKind,
}

async fn create_lecture(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((course_id, section_id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<CreateLecture>,
) -> AppResult<(StatusCode, Json<Lecture>)> {
    let course = editable_course(&state, &current, course_id).await?;
    let section = section_of(&state, &course, section_id).await?;
    let siblings = state.curriculum.lectures(course.id).await?;
    let position = next_position(
        siblings
            .iter()
            .filter(|l| l.section_id == section.id)
            .map(|l| l.position),
    );
    let lecture = Lecture {
        id: Uuid::new_v4(),
        course_id: course.id,
        section_id: section.id,
        title: validate_title(&req.title)?,
        kind: req.kind,
        position,
        is_preview: false,
        article_body: String::new(),
        duration_sec: 0,
        video_asset_id: None,
    };
    state.curriculum.insert_lecture(&lecture).await?;
    Ok((StatusCode::CREATED, Json(lecture)))
}

#[derive(Deserialize)]
struct UpdateLecture {
    title: Option<String>,
    is_preview: Option<bool>,
    article_body: Option<String>,
}

async fn update_lecture(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((course_id, lecture_id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<UpdateLecture>,
) -> AppResult<Json<Lecture>> {
    let course = editable_course(&state, &current, course_id).await?;
    let mut lecture = lecture_of(&state, &course, lecture_id).await?;

    if let Some(title) = req.title {
        lecture.title = validate_title(&title)?;
    }
    if let Some(is_preview) = req.is_preview {
        lecture.is_preview = is_preview;
    }
    if let Some(body) = req.article_body {
        if lecture.kind != LectureKind::Article {
            return Err(AppError::BadRequest(
                "only article lectures have a body".into(),
            ));
        }
        if body.chars().count() > MAX_ARTICLE {
            return Err(AppError::BadRequest(format!(
                "article must be at most {MAX_ARTICLE} characters"
            )));
        }
        lecture.article_body = body;
    }

    state.curriculum.save_lecture(&lecture).await?;
    Ok(Json(lecture))
}

async fn delete_lecture(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((course_id, lecture_id)): ApiPath<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let course = editable_course(&state, &current, course_id).await?;
    let lecture = lecture_of(&state, &course, lecture_id).await?;
    state.curriculum.delete_lecture(lecture.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ReorderBody {
    sections: Vec<ReorderSection>,
}

#[derive(Deserialize)]
struct ReorderSection {
    id: Uuid,
    lecture_ids: Vec<Uuid>,
}

/// Replaces the whole ordering. The body must list every section of the course
/// and every lecture exactly once; lectures may move between sections.
async fn reorder(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(course_id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<ReorderBody>,
) -> AppResult<Json<Vec<SectionWithLectures>>> {
    let course = editable_course(&state, &current, course_id).await?;
    let sections: HashSet<Uuid> = state
        .curriculum
        .sections(course.id)
        .await?
        .iter()
        .map(|s| s.id)
        .collect();
    let lectures: HashSet<Uuid> = state
        .curriculum
        .lectures(course.id)
        .await?
        .iter()
        .map(|l| l.id)
        .collect();

    let given_sections: Vec<Uuid> = req.sections.iter().map(|s| s.id).collect();
    let given_lectures: Vec<Uuid> = req
        .sections
        .iter()
        .flat_map(|s| s.lecture_ids.iter().copied())
        .collect();
    if !is_permutation(&given_sections, &sections) || !is_permutation(&given_lectures, &lectures) {
        return Err(AppError::BadRequest(
            "order must list every section and lecture of the course exactly once".into(),
        ));
    }

    let order: Vec<SectionOrder> = req
        .sections
        .into_iter()
        .map(|s| SectionOrder {
            section_id: s.id,
            lecture_ids: s.lecture_ids,
        })
        .collect();
    state.curriculum.reorder(course.id, &order).await?;
    Ok(Json(load_curriculum(&state, course.id).await?))
}

fn is_permutation(given: &[Uuid], expected: &HashSet<Uuid>) -> bool {
    let unique: HashSet<Uuid> = given.iter().copied().collect();
    unique.len() == given.len() && &unique == expected
}
