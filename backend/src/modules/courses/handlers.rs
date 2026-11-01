use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Deserialize;
use uuid::Uuid;

use super::model::{Course, CourseStatus, Currency, Level};
use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::{ApiJson, ApiPath},
    modules::users::model::{Role, User},
    state::AppState,
};

const MAX_TITLE: usize = 60;
const MAX_SUBTITLE: usize = 120;
const MAX_DESCRIPTION: usize = 20_000;
const MAX_LIST_ITEMS: usize = 20;
const MAX_LIST_ITEM_LEN: usize = 160;
/// Upper bound on price in minor units (e.g. USD 10,000.00 / KHR 100,000,000).
const MAX_PRICE_MINOR: i64 = 100_000_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/courses", get(list_mine).post(create))
        .route("/courses/{id}", get(get_one).patch(update).delete(delete))
}

/// Instructors (and admins) only.
fn require_instructor(current: &CurrentUser) -> AppResult<&User> {
    match current.0.role {
        Role::Instructor | Role::Admin => Ok(&current.0),
        Role::Student => Err(AppError::Forbidden),
    }
}

/// Loads a course the user may manage: their own, or any if admin.
/// Other people's courses are reported as not found (no existence leak).
pub async fn load_managed_course(state: &AppState, user: &User, id: Uuid) -> AppResult<Course> {
    let course = state.courses.find(id).await?.ok_or(AppError::NotFound)?;
    if course.instructor_id == user.id || user.role == Role::Admin {
        Ok(course)
    } else {
        Err(AppError::NotFound)
    }
}

/// Like `load_managed_course`, but refuses courses locked for review.
pub async fn load_editable_course(state: &AppState, user: &User, id: Uuid) -> AppResult<Course> {
    let course = load_managed_course(state, user, id).await?;
    if course.status == CourseStatus::Pending {
        return Err(AppError::Conflict(
            "course is under review and cannot be edited".into(),
        ));
    }
    Ok(course)
}

async fn validate_category(state: &AppState, id: Option<Uuid>) -> AppResult<()> {
    if let Some(id) = id
        && state.categories.find(id).await?.is_none()
    {
        return Err(AppError::BadRequest("category does not exist".into()));
    }
    Ok(())
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

fn validate_text(field: &str, value: &str, max: usize) -> AppResult<String> {
    let value = value.trim().to_string();
    if value.chars().count() > max {
        return Err(AppError::BadRequest(format!(
            "{field} must be at most {max} characters"
        )));
    }
    Ok(value)
}

fn validate_list(field: &str, items: Vec<String>) -> AppResult<Vec<String>> {
    let items: Vec<String> = items
        .into_iter()
        .map(|i| i.trim().to_string())
        .filter(|i| !i.is_empty())
        .collect();
    if items.len() > MAX_LIST_ITEMS {
        return Err(AppError::BadRequest(format!(
            "{field} can have at most {MAX_LIST_ITEMS} items"
        )));
    }
    if items.iter().any(|i| i.chars().count() > MAX_LIST_ITEM_LEN) {
        return Err(AppError::BadRequest(format!(
            "each {field} item must be at most {MAX_LIST_ITEM_LEN} characters"
        )));
    }
    Ok(items)
}

#[derive(Deserialize)]
struct CreateCourse {
    title: String,
    category_id: Option<Uuid>,
}

async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(req): ApiJson<CreateCourse>,
) -> AppResult<(StatusCode, Json<Course>)> {
    let user = require_instructor(&current)?;
    let title = validate_title(&req.title)?;
    validate_category(&state, req.category_id).await?;

    let course = Course::new_draft(
        Uuid::new_v4(),
        user.id,
        title,
        req.category_id,
        state.clock.now(),
    );
    state.courses.insert(&course).await?;
    Ok((StatusCode::CREATED, Json(course)))
}

async fn list_mine(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<Json<Vec<Course>>> {
    let user = require_instructor(&current)?;
    Ok(Json(state.courses.list_by_instructor(user.id).await?))
}

async fn get_one(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<Course>> {
    let user = require_instructor(&current)?;
    Ok(Json(load_managed_course(&state, user, id).await?))
}

/// Partial update: only fields present in the body change.
#[derive(Deserialize)]
struct UpdateCourse {
    title: Option<String>,
    subtitle: Option<String>,
    description: Option<String>,
    category_id: Option<Uuid>,
    level: Option<Level>,
    language: Option<String>,
    price_minor: Option<i64>,
    currency: Option<Currency>,
    outcomes: Option<Vec<String>>,
    requirements: Option<Vec<String>>,
}

async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<UpdateCourse>,
) -> AppResult<Json<Course>> {
    let user = require_instructor(&current)?;
    let mut course = load_editable_course(&state, user, id).await?;

    if let Some(title) = req.title {
        course.title = validate_title(&title)?;
    }
    if let Some(subtitle) = req.subtitle {
        course.subtitle = validate_text("subtitle", &subtitle, MAX_SUBTITLE)?;
    }
    if let Some(description) = req.description {
        course.description = validate_text("description", &description, MAX_DESCRIPTION)?;
    }
    if let Some(category_id) = req.category_id {
        validate_category(&state, Some(category_id)).await?;
        course.category_id = Some(category_id);
    }
    if let Some(level) = req.level {
        course.level = level;
    }
    if let Some(language) = req.language {
        let language = language.trim().to_lowercase();
        if !(2..=10).contains(&language.len()) {
            return Err(AppError::BadRequest(
                "language must be a 2-10 character code".into(),
            ));
        }
        course.language = language;
    }
    if let Some(price) = req.price_minor {
        if !(0..=MAX_PRICE_MINOR).contains(&price) {
            return Err(AppError::BadRequest("price is out of range".into()));
        }
        course.price_minor = price;
    }
    if let Some(currency) = req.currency {
        course.currency = currency;
    }
    if let Some(outcomes) = req.outcomes {
        course.outcomes = validate_list("outcomes", outcomes)?;
    }
    if let Some(requirements) = req.requirements {
        course.requirements = validate_list("requirements", requirements)?;
    }

    course.updated_at = state.clock.now();
    state.courses.save(&course).await?;
    Ok(Json(course))
}

async fn delete(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    let user = require_instructor(&current)?;
    let course = load_managed_course(&state, user, id).await?;
    if course.status != CourseStatus::Draft {
        return Err(AppError::Conflict(
            "only draft courses can be deleted".into(),
        ));
    }
    state.courses.delete(course.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
