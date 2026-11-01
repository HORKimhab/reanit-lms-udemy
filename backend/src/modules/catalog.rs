//! Public course catalog: search/browse published courses and course pages.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    extract::ApiPath,
    modules::{
        courses::{
            model::{Course, Currency, Level},
            repo::{CatalogFilter, CatalogSort, PriceFilter},
        },
        curriculum::{handlers::load_curriculum, model::LectureKind, repo::LectureStats},
    },
    state::AppState,
};

const DEFAULT_PER_PAGE: i64 = 20;
const MAX_PER_PAGE: i64 = 50;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/courses", get(search))
        .route("/courses/{slug}", get(course_page))
}

/// Compact representation used in listings.
#[derive(Serialize)]
pub struct CourseCard {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail_url: Option<String>,
    pub category_id: Option<Uuid>,
    pub level: Level,
    pub language: String,
    pub price_minor: i64,
    pub currency: Currency,
    pub instructor_id: Uuid,
    pub instructor_name: String,
    pub lecture_count: i64,
    pub duration_sec: i64,
    pub published_at: Option<DateTime<Utc>>,
}

/// Builds cards with two batch queries (instructors, lecture stats) per page.
pub async fn to_cards(state: &AppState, courses: Vec<Course>) -> AppResult<Vec<CourseCard>> {
    let ids: Vec<Uuid> = courses.iter().map(|c| c.id).collect();
    let mut instructor_ids: Vec<Uuid> = courses.iter().map(|c| c.instructor_id).collect();
    instructor_ids.sort();
    instructor_ids.dedup();

    let names: HashMap<Uuid, String> = state
        .users
        .find_many(&instructor_ids)
        .await?
        .into_iter()
        .map(|u| (u.id, u.name))
        .collect();
    let stats = state.curriculum.stats(&ids).await?;

    Ok(courses
        .into_iter()
        .map(|c| {
            let s = stats.get(&c.id).copied().unwrap_or_default();
            card(state, c, names.clone(), s)
        })
        .collect())
}

fn card(
    state: &AppState,
    c: Course,
    names: HashMap<Uuid, String>,
    stats: LectureStats,
) -> CourseCard {
    CourseCard {
        thumbnail_url: c
            .thumbnail_key
            .as_deref()
            .map(|k| state.storage.public_url(k)),
        instructor_name: names.get(&c.instructor_id).cloned().unwrap_or_default(),
        lecture_count: stats.lecture_count,
        duration_sec: stats.duration_sec,
        id: c.id,
        slug: c.slug,
        title: c.title,
        subtitle: c.subtitle,
        category_id: c.category_id,
        level: c.level,
        language: c.language,
        price_minor: c.price_minor,
        currency: c.currency,
        instructor_id: c.instructor_id,
        published_at: c.published_at,
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
    /// Category slug.
    category: Option<String>,
    level: Option<Level>,
    price: Option<PriceFilter>,
    sort: Option<CatalogSort>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Serialize)]
struct PageMeta {
    page: i64,
    per_page: i64,
    total: i64,
}

#[derive(Serialize)]
struct SearchResponse {
    data: Vec<CourseCard>,
    meta: PageMeta,
}

async fn search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<SearchResponse>> {
    let page = q.page.unwrap_or(1);
    let per_page = q.per_page.unwrap_or(DEFAULT_PER_PAGE);
    if page < 1 || !(1..=MAX_PER_PAGE).contains(&per_page) {
        return Err(AppError::BadRequest(format!(
            "page must be >= 1 and per_page between 1 and {MAX_PER_PAGE}"
        )));
    }
    let empty = || {
        Json(SearchResponse {
            data: vec![],
            meta: PageMeta {
                page,
                per_page,
                total: 0,
            },
        })
    };

    let category_id = match q.category.as_deref() {
        None | Some("") => None,
        Some(slug) => match state
            .categories
            .list()
            .await?
            .into_iter()
            .find(|c| c.slug == slug)
        {
            Some(c) => Some(c.id),
            None => return Ok(empty()),
        },
    };
    let text = q.q.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let sort = q.sort.unwrap_or(if text.is_some() {
        CatalogSort::Relevance
    } else {
        CatalogSort::Newest
    });

    let filter = CatalogFilter {
        q: text,
        category_id,
        level: q.level,
        price: q.price,
        sort,
        limit: per_page,
        offset: (page - 1) * per_page,
    };
    let (courses, total) = state.courses.search_published(&filter).await?;
    Ok(Json(SearchResponse {
        data: to_cards(&state, courses).await?,
        meta: PageMeta {
            page,
            per_page,
            total,
        },
    }))
}

#[derive(Serialize)]
struct PublicInstructor {
    id: Uuid,
    name: String,
    avatar_url: Option<String>,
    headline: String,
    bio: String,
}

#[derive(Serialize)]
struct PublicLecture {
    id: Uuid,
    title: String,
    kind: LectureKind,
    duration_sec: i32,
    is_preview: bool,
    /// Only for free-preview article lectures.
    #[serde(skip_serializing_if = "Option::is_none")]
    article_body: Option<String>,
    /// Only for free-preview video lectures.
    #[serde(skip_serializing_if = "Option::is_none")]
    preview_asset_id: Option<Uuid>,
}

#[derive(Serialize)]
struct PublicSection {
    id: Uuid,
    title: String,
    lectures: Vec<PublicLecture>,
}

#[derive(Serialize)]
struct CoursePage {
    #[serde(flatten)]
    card: CourseCard,
    description: String,
    outcomes: Vec<String>,
    requirements: Vec<String>,
    instructor: PublicInstructor,
    curriculum: Vec<PublicSection>,
}

async fn course_page(
    State(state): State<AppState>,
    ApiPath(slug): ApiPath<String>,
) -> AppResult<Json<CoursePage>> {
    let course = state
        .courses
        .find_published_by_slug(&slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let instructor = state
        .users
        .find_by_id(course.instructor_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let profile = state.instructor_profiles.find(instructor.id).await?;

    let curriculum = load_curriculum(&state, course.id)
        .await?
        .into_iter()
        .map(|s| PublicSection {
            id: s.section.id,
            title: s.section.title,
            lectures: s
                .lectures
                .into_iter()
                .map(|l| PublicLecture {
                    article_body: (l.is_preview && l.kind == LectureKind::Article)
                        .then(|| l.article_body.clone()),
                    preview_asset_id: if l.is_preview { l.video_asset_id } else { None },
                    id: l.id,
                    title: l.title,
                    kind: l.kind,
                    duration_sec: l.duration_sec,
                    is_preview: l.is_preview,
                })
                .collect(),
        })
        .collect();

    let (description, outcomes, requirements) = (
        course.description.clone(),
        course.outcomes.clone(),
        course.requirements.clone(),
    );
    let card = to_cards(&state, vec![course]).await?.remove(0);
    Ok(Json(CoursePage {
        card,
        description,
        outcomes,
        requirements,
        instructor: PublicInstructor {
            id: instructor.id,
            name: instructor.name,
            avatar_url: instructor.avatar_url,
            headline: profile
                .as_ref()
                .map(|p| p.headline.clone())
                .unwrap_or_default(),
            bio: profile.map(|p| p.bio).unwrap_or_default(),
        },
        curriculum,
    }))
}
