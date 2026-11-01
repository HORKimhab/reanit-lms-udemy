use async_trait::async_trait;
use redis::aio::ConnectionManager;
use sqlx::PgPool;

use crate::error::AppResult;

#[async_trait]
pub trait HealthProbe: Send + Sync {
    async fn check(&self) -> AppResult<()>;
}

pub struct PgRedisProbe {
    pub db: PgPool,
    pub redis: ConnectionManager,
}

#[async_trait]
impl HealthProbe for PgRedisProbe {
    async fn check(&self) -> AppResult<()> {
        sqlx::query("SELECT 1").execute(&self.db).await?;
        let mut redis = self.redis.clone();
        let _: String = redis::cmd("PING").query_async(&mut redis).await?;
        Ok(())
    }
}
