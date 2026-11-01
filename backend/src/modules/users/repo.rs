use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::model::{NewUser, Role, User};
use crate::error::AppResult;

#[async_trait]
pub trait UserRepo: Send + Sync {
    /// Fails with `AppError::Conflict` if the email is already registered.
    async fn create(&self, new: NewUser, now: DateTime<Utc>) -> AppResult<User>;
    /// Email lookup is case-insensitive.
    async fn find_by_email(&self, email: &str) -> AppResult<Option<User>>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<User>>;
    async fn find_many(&self, ids: &[Uuid]) -> AppResult<Vec<User>>;
    async fn mark_email_verified(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<Option<User>>;
    async fn set_role(&self, id: Uuid, role: Role) -> AppResult<Option<User>>;
}
