//! Postgres adapters for the repository traits.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    auth::store::{RefreshToken, RefreshTokenRepo, VerificationTokenRepo},
    error::{AppError, AppResult},
    modules::{
        categories::{Category, CategoryRepo},
        courses::{
            model::{Course, CourseStatus, Currency, Level},
            repo::CourseRepo,
        },
        instructors::{InstructorProfile, InstructorProfileRepo},
        users::{
            model::{NewUser, Role, User},
            repo::UserRepo,
        },
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

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM refresh_tokens WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl VerificationTokenRepo for PgStore {
    async fn replace_for_user(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM email_verification_tokens WHERE user_id = $1 AND used_at IS NULL")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO email_verification_tokens (user_id, token_hash, expires_at, created_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn last_issued_at(&self, user_id: Uuid) -> AppResult<Option<DateTime<Utc>>> {
        Ok(sqlx::query_scalar(
            "SELECT max(created_at) FROM email_verification_tokens WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?)
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

#[derive(FromRow)]
struct CategoryRow {
    id: Uuid,
    parent_id: Option<Uuid>,
    name: String,
    slug: String,
}

impl From<CategoryRow> for Category {
    fn from(r: CategoryRow) -> Self {
        Category {
            id: r.id,
            parent_id: r.parent_id,
            name: r.name,
            slug: r.slug,
        }
    }
}

#[async_trait]
impl CategoryRepo for PgStore {
    async fn list(&self) -> AppResult<Vec<Category>> {
        let rows: Vec<CategoryRow> = sqlx::query_as(
            "SELECT id, parent_id, name, slug FROM categories ORDER BY position, name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find(&self, id: Uuid) -> AppResult<Option<Category>> {
        let row: Option<CategoryRow> =
            sqlx::query_as("SELECT id, parent_id, name, slug FROM categories WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(Into::into))
    }
}

#[derive(FromRow)]
struct InstructorProfileRow {
    user_id: Uuid,
    headline: String,
    bio: String,
    website: Option<String>,
}

impl From<InstructorProfileRow> for InstructorProfile {
    fn from(r: InstructorProfileRow) -> Self {
        InstructorProfile {
            user_id: r.user_id,
            headline: r.headline,
            bio: r.bio,
            website: r.website,
        }
    }
}

#[async_trait]
impl InstructorProfileRepo for PgStore {
    async fn find(&self, user_id: Uuid) -> AppResult<Option<InstructorProfile>> {
        let row: Option<InstructorProfileRow> = sqlx::query_as(
            "SELECT user_id, headline, bio, website FROM instructor_profiles WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn upsert(&self, p: InstructorProfile) -> AppResult<InstructorProfile> {
        let row: InstructorProfileRow = sqlx::query_as(
            "INSERT INTO instructor_profiles (user_id, headline, bio, website)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id) DO UPDATE
                SET headline = EXCLUDED.headline, bio = EXCLUDED.bio, website = EXCLUDED.website
             RETURNING user_id, headline, bio, website",
        )
        .bind(p.user_id)
        .bind(&p.headline)
        .bind(&p.bio)
        .bind(&p.website)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }
}

macro_rules! course_columns {
    () => {
        "id, instructor_id, category_id, title, slug, subtitle, description, level, language, \
         price_minor, currency, thumbnail_key, outcomes, requirements, status, rejection_reason, \
         created_at, updated_at, published_at"
    };
}

#[derive(FromRow)]
struct CourseRow {
    id: Uuid,
    instructor_id: Uuid,
    category_id: Option<Uuid>,
    title: String,
    slug: String,
    subtitle: String,
    description: String,
    level: String,
    language: String,
    price_minor: i64,
    currency: String,
    thumbnail_key: Option<String>,
    outcomes: Vec<String>,
    requirements: Vec<String>,
    status: String,
    rejection_reason: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    published_at: Option<DateTime<Utc>>,
}

fn parse_text<T: std::str::FromStr<Err = String>>(s: &str) -> AppResult<T> {
    s.parse().map_err(|e: String| anyhow::anyhow!(e).into())
}

impl TryFrom<CourseRow> for Course {
    type Error = AppError;
    fn try_from(r: CourseRow) -> AppResult<Self> {
        Ok(Course {
            id: r.id,
            instructor_id: r.instructor_id,
            category_id: r.category_id,
            title: r.title,
            slug: r.slug,
            subtitle: r.subtitle,
            description: r.description,
            level: parse_text::<Level>(&r.level)?,
            language: r.language,
            price_minor: r.price_minor,
            currency: parse_text::<Currency>(&r.currency)?,
            thumbnail_key: r.thumbnail_key,
            outcomes: r.outcomes,
            requirements: r.requirements,
            status: parse_text::<CourseStatus>(&r.status)?,
            rejection_reason: r.rejection_reason,
            created_at: r.created_at,
            updated_at: r.updated_at,
            published_at: r.published_at,
        })
    }
}

#[async_trait]
impl CourseRepo for PgStore {
    async fn insert(&self, c: &Course) -> AppResult<()> {
        sqlx::query(concat!(
            "INSERT INTO courses (",
            course_columns!(),
            ") VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)"
        ))
        .bind(c.id)
        .bind(c.instructor_id)
        .bind(c.category_id)
        .bind(&c.title)
        .bind(&c.slug)
        .bind(&c.subtitle)
        .bind(&c.description)
        .bind(c.level.as_str())
        .bind(&c.language)
        .bind(c.price_minor)
        .bind(c.currency.as_str())
        .bind(&c.thumbnail_key)
        .bind(&c.outcomes)
        .bind(&c.requirements)
        .bind(c.status.as_str())
        .bind(&c.rejection_reason)
        .bind(c.created_at)
        .bind(c.updated_at)
        .bind(c.published_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find(&self, id: Uuid) -> AppResult<Option<Course>> {
        let row: Option<CourseRow> = sqlx::query_as(concat!(
            "SELECT ",
            course_columns!(),
            " FROM courses WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Course::try_from).transpose()
    }

    async fn list_by_instructor(&self, instructor_id: Uuid) -> AppResult<Vec<Course>> {
        let rows: Vec<CourseRow> = sqlx::query_as(concat!(
            "SELECT ",
            course_columns!(),
            " FROM courses WHERE instructor_id = $1 ORDER BY created_at DESC"
        ))
        .bind(instructor_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Course::try_from).collect()
    }

    async fn save(&self, c: &Course) -> AppResult<()> {
        sqlx::query(
            "UPDATE courses SET category_id = $2, title = $3, subtitle = $4, description = $5,
                level = $6, language = $7, price_minor = $8, currency = $9, thumbnail_key = $10,
                outcomes = $11, requirements = $12, status = $13, rejection_reason = $14,
                updated_at = $15, published_at = $16
             WHERE id = $1",
        )
        .bind(c.id)
        .bind(c.category_id)
        .bind(&c.title)
        .bind(&c.subtitle)
        .bind(&c.description)
        .bind(c.level.as_str())
        .bind(&c.language)
        .bind(c.price_minor)
        .bind(c.currency.as_str())
        .bind(&c.thumbnail_key)
        .bind(&c.outcomes)
        .bind(&c.requirements)
        .bind(c.status.as_str())
        .bind(&c.rejection_reason)
        .bind(c.updated_at)
        .bind(c.published_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM courses WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
