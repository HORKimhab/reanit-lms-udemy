use async_trait::async_trait;
use uuid::Uuid;

use super::model::{Course, CourseStatus, Level};
use crate::error::AppResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceFilter {
    Free,
    Paid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSort {
    Relevance,
    Newest,
    PriceAsc,
    PriceDesc,
}

impl CatalogSort {
    pub fn as_str(self) -> &'static str {
        match self {
            CatalogSort::Relevance => "relevance",
            CatalogSort::Newest => "newest",
            CatalogSort::PriceAsc => "price_asc",
            CatalogSort::PriceDesc => "price_desc",
        }
    }
}

/// KHR per USD used only to sort mixed-currency prices.
pub const KHR_PER_USD: i64 = 4100;

/// Price in approximate USD cents, for cross-currency sorting.
pub fn usd_cents_equivalent(course: &Course) -> f64 {
    match course.currency {
        super::model::Currency::Usd => course.price_minor as f64,
        super::model::Currency::Khr => course.price_minor as f64 * 100.0 / KHR_PER_USD as f64,
    }
}

/// Filters for the public catalog (published courses only).
#[derive(Clone, Debug)]
pub struct CatalogFilter {
    pub q: Option<String>,
    pub category_id: Option<Uuid>,
    pub level: Option<Level>,
    pub price: Option<PriceFilter>,
    pub sort: CatalogSort,
    pub limit: i64,
    pub offset: i64,
}

#[async_trait]
pub trait CourseRepo: Send + Sync {
    async fn insert(&self, course: &Course) -> AppResult<()>;
    async fn find(&self, id: Uuid) -> AppResult<Option<Course>>;
    /// Any order; missing ids are skipped.
    async fn find_many(&self, ids: &[Uuid]) -> AppResult<Vec<Course>>;
    /// Newest first.
    async fn list_by_instructor(&self, instructor_id: Uuid) -> AppResult<Vec<Course>>;
    /// Persists every mutable field of an existing course.
    async fn save(&self, course: &Course) -> AppResult<()>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    /// Oldest first (review queue order).
    async fn list_by_status(&self, status: CourseStatus) -> AppResult<Vec<Course>>;
    /// A page of published courses plus the total number of matches.
    async fn search_published(&self, filter: &CatalogFilter) -> AppResult<(Vec<Course>, i64)>;
    async fn find_published_by_slug(&self, slug: &str) -> AppResult<Option<Course>>;
}
