//! Instructor coupons: a percentage off one course, optionally time- or
//! use-limited. Applied in the cart and at checkout.

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
        cart::{Total, totals_of},
        courses::{
            handlers::{load_managed_course, require_instructor},
            model::Course,
        },
    },
    state::AppState,
};

#[derive(Clone, Debug, Serialize)]
pub struct Coupon {
    pub id: Uuid,
    pub course_id: Uuid,
    /// Uppercase.
    pub code: String,
    pub discount_percent: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_uses: Option<i32>,
    pub used_count: i32,
    pub created_at: DateTime<Utc>,
}

impl Coupon {
    pub fn is_usable(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_none_or(|t| now < t)
            && self.max_uses.is_none_or(|max| self.used_count < max)
    }

    /// Price after the discount, rounded in the buyer's favour.
    pub fn apply(&self, price_minor: i64) -> i64 {
        let discount = (price_minor * i64::from(self.discount_percent) + 99) / 100;
        (price_minor - discount).max(0)
    }
}

#[async_trait]
pub trait CouponRepo: Send + Sync {
    /// `Conflict` if the course already has this code.
    async fn insert(&self, coupon: &Coupon) -> AppResult<()>;
    /// Newest first.
    async fn list_by_course(&self, course_id: Uuid) -> AppResult<Vec<Coupon>>;
    /// Coupons of any of these courses whose code is one of `codes` (uppercase).
    async fn find_for(&self, course_ids: &[Uuid], codes: &[String]) -> AppResult<Vec<Coupon>>;
    async fn delete(&self, course_id: Uuid, id: Uuid) -> AppResult<bool>;
}

/// Normalizes user-typed codes: trimmed, uppercase, empties dropped.
pub fn normalize_codes<'a>(codes: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = codes
        .into_iter()
        .map(|c| c.trim().to_uppercase())
        .filter(|c| !c.is_empty())
        .collect();
    out.dedup();
    out
}

/// One course's price after coupons.
#[derive(Clone, Debug, Serialize)]
pub struct PricedLine {
    pub course_id: Uuid,
    pub list_price_minor: i64,
    pub price_minor: i64,
    pub coupon_code: Option<String>,
    #[serde(skip)]
    pub coupon_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct Pricing {
    pub lines: Vec<PricedLine>,
    pub totals: Vec<Total>,
    /// Requested codes that didn't discount anything.
    pub invalid_codes: Vec<String>,
}

/// Prices courses with the best usable coupon per course.
pub async fn price(state: &AppState, courses: &[Course], codes: &[String]) -> AppResult<Pricing> {
    let now = state.clock.now();
    let ids: Vec<Uuid> = courses.iter().map(|c| c.id).collect();
    let coupons: Vec<Coupon> = if codes.is_empty() {
        vec![]
    } else {
        state.coupons.find_for(&ids, codes).await?
    };

    let lines: Vec<PricedLine> = courses
        .iter()
        .map(|c| {
            let best = coupons
                .iter()
                .filter(|k| k.course_id == c.id && k.is_usable(now))
                .max_by_key(|k| k.discount_percent);
            PricedLine {
                course_id: c.id,
                list_price_minor: c.price_minor,
                price_minor: best.map_or(c.price_minor, |k| k.apply(c.price_minor)),
                coupon_code: best.map(|k| k.code.clone()),
                coupon_id: best.map(|k| k.id),
            }
        })
        .collect();
    let invalid_codes = codes
        .iter()
        .filter(|code| !lines.iter().any(|l| l.coupon_code.as_ref() == Some(*code)))
        .cloned()
        .collect();
    let totals = totals_of(
        courses
            .iter()
            .zip(&lines)
            .map(|(c, l)| (c.currency, l.price_minor)),
    );
    Ok(Pricing {
        lines,
        totals,
        invalid_codes,
    })
}

// ---------- instructor endpoints (mounted under /instructor) ----------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/courses/{id}/coupons", get(list).post(create))
        .route("/courses/{id}/coupons/{coupon_id}", delete(remove))
}

#[derive(Deserialize)]
struct CouponInput {
    code: String,
    discount_percent: i32,
    expires_at: Option<DateTime<Utc>>,
    max_uses: Option<i32>,
}

#[derive(Serialize)]
struct CouponList {
    data: Vec<Coupon>,
}

async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(course_id): ApiPath<Uuid>,
    ApiJson(input): ApiJson<CouponInput>,
) -> AppResult<(StatusCode, Json<Coupon>)> {
    let user = require_instructor(&current)?;
    let course = load_managed_course(&state, user, course_id).await?;

    let code = input.code.trim().to_uppercase();
    let valid_code = (3..=20).contains(&code.len())
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid_code {
        return Err(AppError::BadRequest(
            "code must be 3-20 letters, digits, '-' or '_'".into(),
        ));
    }
    if !(1..=100).contains(&input.discount_percent) {
        return Err(AppError::BadRequest(
            "discount_percent must be between 1 and 100".into(),
        ));
    }
    if input.max_uses.is_some_and(|m| m < 1) {
        return Err(AppError::BadRequest("max_uses must be at least 1".into()));
    }

    let coupon = Coupon {
        id: Uuid::new_v4(),
        course_id: course.id,
        code,
        discount_percent: input.discount_percent,
        expires_at: input.expires_at,
        max_uses: input.max_uses,
        used_count: 0,
        created_at: state.clock.now(),
    };
    state.coupons.insert(&coupon).await?;
    Ok((StatusCode::CREATED, Json(coupon)))
}

async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(course_id): ApiPath<Uuid>,
) -> AppResult<Json<CouponList>> {
    let user = require_instructor(&current)?;
    let course = load_managed_course(&state, user, course_id).await?;
    Ok(Json(CouponList {
        data: state.coupons.list_by_course(course.id).await?,
    }))
}

async fn remove(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((course_id, coupon_id)): ApiPath<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let user = require_instructor(&current)?;
    let course = load_managed_course(&state, user, course_id).await?;
    if state.coupons.delete(course.id, coupon_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}
