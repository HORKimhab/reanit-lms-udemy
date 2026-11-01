use async_trait::async_trait;
use uuid::Uuid;

use super::model::{Lecture, Section};
use crate::error::AppResult;

/// New order for one section: its position is its index in the list.
pub struct SectionOrder {
    pub section_id: Uuid,
    pub lecture_ids: Vec<Uuid>,
}

#[async_trait]
pub trait CurriculumRepo: Send + Sync {
    async fn sections(&self, course_id: Uuid) -> AppResult<Vec<Section>>;
    async fn lectures(&self, course_id: Uuid) -> AppResult<Vec<Lecture>>;
    async fn find_section(&self, id: Uuid) -> AppResult<Option<Section>>;
    async fn find_lecture(&self, id: Uuid) -> AppResult<Option<Lecture>>;
    async fn insert_section(&self, section: &Section) -> AppResult<()>;
    async fn save_section(&self, section: &Section) -> AppResult<()>;
    /// Also deletes the section's lectures.
    async fn delete_section(&self, id: Uuid) -> AppResult<()>;
    async fn insert_lecture(&self, lecture: &Lecture) -> AppResult<()>;
    async fn save_lecture(&self, lecture: &Lecture) -> AppResult<()>;
    async fn delete_lecture(&self, id: Uuid) -> AppResult<()>;
    /// Atomically applies a complete, already-validated ordering.
    async fn reorder(&self, course_id: Uuid, order: &[SectionOrder]) -> AppResult<()>;
}
