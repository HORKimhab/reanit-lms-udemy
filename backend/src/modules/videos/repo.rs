use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::model::VideoAsset;
use crate::error::AppResult;

#[async_trait]
pub trait VideoAssetRepo: Send + Sync {
    async fn insert(&self, asset: &VideoAsset) -> AppResult<()>;
    async fn find(&self, id: Uuid) -> AppResult<Option<VideoAsset>>;
    async fn save(&self, asset: &VideoAsset) -> AppResult<()>;
    /// Atomically claims the oldest processing job that is unclaimed, or whose
    /// claim is older than `stale_before` (a crashed worker). Increments `attempts`.
    async fn claim_next(
        &self,
        now: DateTime<Utc>,
        stale_before: DateTime<Utc>,
    ) -> AppResult<Option<VideoAsset>>;
}
