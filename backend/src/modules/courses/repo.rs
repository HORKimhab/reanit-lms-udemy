use async_trait::async_trait;
use uuid::Uuid;

use super::model::{Course, CourseStatus};
use crate::error::AppResult;

#[async_trait]
pub trait CourseRepo: Send + Sync {
    async fn insert(&self, course: &Course) -> AppResult<()>;
    async fn find(&self, id: Uuid) -> AppResult<Option<Course>>;
    /// Newest first.
    async fn list_by_instructor(&self, instructor_id: Uuid) -> AppResult<Vec<Course>>;
    /// Persists every mutable field of an existing course.
    async fn save(&self, course: &Course) -> AppResult<()>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    /// Oldest first (review queue order).
    async fn list_by_status(&self, status: CourseStatus) -> AppResult<Vec<Course>>;
}
