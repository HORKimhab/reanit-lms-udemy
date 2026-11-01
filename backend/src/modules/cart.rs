//! Cart and wishlist: per-student lists of saved courses.

use std::collections::BTreeMap;

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{delete, get},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::{ApiJson, ApiPath},
    modules::{
        catalog::{CourseCard, to_cards},
        courses::model::{Course, CourseStatus, Currency},
    },
    state::AppState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedList {
    /// Paid courses the student intends to buy.
    Cart,
    /// Any published course, bookmarked for later.
    Wishlist,
}

#[async_trait]
pub trait SavedCourseRepo: Send + Sync {
    /// Idempotent.
    async fn add(
        &self,
        list: SavedList,
        user_id: Uuid,
        course_id: Uuid,
        at: DateTime<Utc>,
    ) -> AppResult<()>;
    async fn remove(&self, list: SavedList, user_id: Uuid, course_id: Uuid) -> AppResult<()>;
    /// Course ids, most recently added first.
    async fn list(&self, list: SavedList, user_id: Uuid) -> AppResult<Vec<Uuid>>;
}

/// Mounted under `/me`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/cart", get(get_cart).post(add_to_cart))
        .route("/cart/{course_id}", delete(remove_from_cart))
        .route("/wishlist", get(get_wishlist).post(add_to_wishlist))
        .route("/wishlist/{course_id}", delete(remove_from_wishlist))
}

#[derive(Deserialize)]
struct AddInput {
    course_id: Uuid,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct Total {
    pub currency: Currency,
    pub amount_minor: i64,
}

#[derive(Serialize)]
struct CartView {
    data: Vec<CourseCard>,
    /// One entry per currency, ordered by currency code.
    totals: Vec<Total>,
}

#[derive(Serialize)]
struct WishlistView {
    data: Vec<CourseCard>,
}

async fn published(state: &AppState, course_id: Uuid) -> AppResult<Course> {
    state
        .courses
        .find(course_id)
        .await?
        .filter(|c| c.status == CourseStatus::Published)
        .ok_or(AppError::NotFound)
}

/// Published courses in the list, in list order.
pub async fn saved_courses(
    state: &AppState,
    list: SavedList,
    user_id: Uuid,
) -> AppResult<Vec<Course>> {
    let ids = state.saved_courses.list(list, user_id).await?;
    let mut found = state.courses.find_many(&ids).await?;
    Ok(ids
        .iter()
        .filter_map(|id| {
            let pos = found
                .iter()
                .position(|c| c.id == *id && c.status == CourseStatus::Published)?;
            Some(found.swap_remove(pos))
        })
        .collect())
}

pub fn totals(courses: &[Course]) -> Vec<Total> {
    let mut sums: BTreeMap<&'static str, (Currency, i64)> = BTreeMap::new();
    for c in courses {
        sums.entry(c.currency.as_str()).or_insert((c.currency, 0)).1 += c.price_minor;
    }
    sums.into_values()
        .map(|(currency, amount_minor)| Total {
            currency,
            amount_minor,
        })
        .collect()
}

async fn get_cart(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<CartView>> {
    let courses = saved_courses(&state, SavedList::Cart, user.id).await?;
    let totals = totals(&courses);
    Ok(Json(CartView {
        data: to_cards(&state, courses).await?,
        totals,
    }))
}

async fn add_to_cart(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(input): ApiJson<AddInput>,
) -> AppResult<StatusCode> {
    let course = published(&state, input.course_id).await?;
    if state.enrollments.is_enrolled(user.id, course.id).await? {
        return Err(AppError::Conflict("you already have this course".into()));
    }
    if course.price_minor == 0 {
        return Err(AppError::BadRequest(
            "free courses don't need checkout; join them directly".into(),
        ));
    }
    if course.instructor_id == user.id {
        return Err(AppError::BadRequest("you can't buy your own course".into()));
    }
    state
        .saved_courses
        .add(SavedList::Cart, user.id, course.id, state.clock.now())
        .await?;
    Ok(StatusCode::OK)
}

async fn remove_from_cart(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(course_id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    state
        .saved_courses
        .remove(SavedList::Cart, user.id, course_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_wishlist(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<WishlistView>> {
    let courses = saved_courses(&state, SavedList::Wishlist, user.id).await?;
    Ok(Json(WishlistView {
        data: to_cards(&state, courses).await?,
    }))
}

async fn add_to_wishlist(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(input): ApiJson<AddInput>,
) -> AppResult<StatusCode> {
    let course = published(&state, input.course_id).await?;
    state
        .saved_courses
        .add(SavedList::Wishlist, user.id, course.id, state.clock.now())
        .await?;
    Ok(StatusCode::OK)
}

async fn remove_from_wishlist(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(course_id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    state
        .saved_courses
        .remove(SavedList::Wishlist, user.id, course_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
