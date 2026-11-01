use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::AppResult;

#[derive(Clone, Debug)]
pub struct RefreshToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Refresh tokens are stored only as SHA-256 hashes.
#[async_trait]
pub trait RefreshTokenRepo: Send + Sync {
    async fn insert(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> AppResult<()>;
    async fn find_by_hash(&self, token_hash: &str) -> AppResult<Option<RefreshToken>>;
    /// Revokes the token if still active. Returns false if it was already revoked
    /// (so concurrent refreshes with the same token can't both succeed).
    async fn revoke_if_active(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<bool>;
    async fn revoke_all_for_user(&self, user_id: Uuid, at: DateTime<Utc>) -> AppResult<()>;
    /// Removes the token entirely (logout). A deleted token presented later is
    /// simply unknown, unlike a revoked one which signals theft.
    async fn delete(&self, id: Uuid) -> AppResult<()>;
}

/// Email verification tokens, stored only as SHA-256 hashes.
#[async_trait]
pub trait VerificationTokenRepo: Send + Sync {
    /// Stores a new token for the user, invalidating any unused older ones.
    async fn replace_for_user(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> AppResult<()>;
    /// Marks the token used and returns its user id, if it exists, is unused and
    /// has not expired at `now`. Single use.
    async fn consume(&self, token_hash: &str, now: DateTime<Utc>) -> AppResult<Option<Uuid>>;
    /// When the user's most recent token was issued, if any.
    async fn last_issued_at(&self, user_id: Uuid) -> AppResult<Option<DateTime<Utc>>>;
}
