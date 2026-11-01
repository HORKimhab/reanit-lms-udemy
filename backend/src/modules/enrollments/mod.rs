//! Enrollments: a student's access to a course. Free courses are joined
//! directly; paid courses are enrolled by the payment flow.

pub mod handlers;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::AppResult;

#[derive(Clone, Debug, Serialize)]
pub struct Enrollment {
    pub user_id: Uuid,
    pub course_id: Uuid,
    pub order_id: Option<Uuid>,
    pub enrolled_at: DateTime<Utc>,
}

#[async_trait]
pub trait EnrollmentRepo: Send + Sync {
    /// Idempotent: enrolling twice keeps the first enrollment.
    async fn enroll(&self, enrollment: &Enrollment) -> AppResult<()>;
    async fn is_enrolled(&self, user_id: Uuid, course_id: Uuid) -> AppResult<bool>;
    /// Newest first.
    async fn list_by_user(&self, user_id: Uuid) -> AppResult<Vec<Enrollment>>;
}
