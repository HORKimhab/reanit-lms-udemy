//! Postgres adapters for the repository traits.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    auth::store::{RefreshToken, RefreshTokenRepo, VerificationTokenRepo},
    error::{AppError, AppResult},
    modules::users::{
        model::{NewUser, Role, User},
        repo::UserRepo,
    },
};

#[derive(Clone)]
pub struct PgStore {
    pub pool: PgPool,
}

impl PgStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Column list shared by user queries (a macro so it works with `concat!`,
/// keeping every SQL string a compile-time constant).
macro_rules! user_columns {
    () => {
        "id, name, email::text AS email, password_hash, role, email_verified_at, avatar_url, created_at"
    };
}

#[derive(FromRow)]
struct UserRow {
    id: Uuid,
    name: String,
    email: String,
    password_hash: String,
    role: String,
    email_verified_at: Option<DateTime<Utc>>,
    avatar_url: Option<String>,
    created_at: DateTime<Utc>,
}

impl TryFrom<UserRow> for User {
    type Error = AppError;
    fn try_from(r: UserRow) -> AppResult<Self> {
        Ok(User {
            id: r.id,
            name: r.name,
            email: r.email,
            password_hash: r.password_hash,
            role: r.role.parse::<Role>().map_err(|e| anyhow::anyhow!(e))?,
            email_verified_at: r.email_verified_at,
            avatar_url: r.avatar_url,
            created_at: r.created_at,
        })
    }
}

fn to_user(row: Option<UserRow>) -> AppResult<Option<User>> {
    row.map(User::try_from).transpose()
}

#[async_trait]
impl UserRepo for PgStore {
    async fn create(&self, new: NewUser, now: DateTime<Utc>) -> AppResult<User> {
        let sql = concat!(
            "INSERT INTO users (name, email, password_hash, created_at, updated_at)
             VALUES ($1, $2::citext, $3, $4, $4) RETURNING ",
            user_columns!()
        );
        let row = sqlx::query_as::<_, UserRow>(sql)
            .bind(&new.name)
            .bind(&new.email)
            .bind(&new.password_hash)
            .bind(now)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match &e {
                sqlx::Error::Database(db) if db.is_unique_violation() => {
                    AppError::Conflict("email is already registered".into())
                }
                _ => e.into(),
            })?;
        row.try_into()
    }

    async fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let sql = concat!(
            "SELECT ",
            user_columns!(),
            " FROM users WHERE email = $1::citext"
        );
        to_user(
            sqlx::query_as(sql)
                .bind(email)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<User>> {
        let sql = concat!("SELECT ", user_columns!(), " FROM users WHERE id = $1");
        to_user(
            sqlx::query_as(sql)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    async fn mark_email_verified(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<Option<User>> {
        let sql = concat!(
            "UPDATE users SET email_verified_at = COALESCE(email_verified_at, $2)
             WHERE id = $1 RETURNING ",
            user_columns!()
        );
        to_user(
            sqlx::query_as(sql)
                .bind(id)
                .bind(at)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    async fn set_role(&self, id: Uuid, role: Role) -> AppResult<Option<User>> {
        let sql = concat!(
            "UPDATE users SET role = $2 WHERE id = $1 RETURNING ",
            user_columns!()
        );
        to_user(
            sqlx::query_as(sql)
                .bind(id)
                .bind(role.as_str())
                .fetch_optional(&self.pool)
                .await?,
        )
    }
}

#[derive(FromRow)]
struct RefreshTokenRow {
    id: Uuid,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

#[async_trait]
impl RefreshTokenRepo for PgStore {
    async fn insert(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO refresh_tokens (user_id, token_hash, expires_at, created_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_hash(&self, token_hash: &str) -> AppResult<Option<RefreshToken>> {
        let row: Option<RefreshTokenRow> = sqlx::query_as(
            "SELECT id, user_id, expires_at, revoked_at FROM refresh_tokens WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| RefreshToken {
            id: r.id,
            user_id: r.user_id,
            expires_at: r.expires_at,
            revoked_at: r.revoked_at,
        }))
    }

    async fn revoke_if_active(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<bool> {
        let res = sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = $2 WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() == 1)
    }

    async fn revoke_all_for_user(&self, user_id: Uuid, at: DateTime<Utc>) -> AppResult<()> {
        sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = $2 WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[async_trait]
impl VerificationTokenRepo for PgStore {
    async fn insert(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO email_verification_tokens (user_id, token_hash, expires_at, created_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn consume(&self, token_hash: &str, now: DateTime<Utc>) -> AppResult<Option<Uuid>> {
        let user_id: Option<Uuid> = sqlx::query_scalar(
            "UPDATE email_verification_tokens SET used_at = $2
             WHERE token_hash = $1 AND used_at IS NULL AND expires_at > $2
             RETURNING user_id",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(user_id)
    }
}
