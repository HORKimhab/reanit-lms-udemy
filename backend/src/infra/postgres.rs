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
            repo::{CatalogFilter, CourseRepo, KHR_PER_USD, PriceFilter},
        },
        curriculum::{
            model::{Lecture, LectureKind, Section},
            repo::{CurriculumRepo, LectureStats, SectionOrder},
        },
        instructors::{InstructorProfile, InstructorProfileRepo},
        users::{
            model::{NewUser, Role, User},
            repo::UserRepo,
        },
        videos::{
            model::{VideoAsset, VideoStatus},
            repo::VideoAssetRepo,
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

    async fn find_many(&self, ids: &[Uuid]) -> AppResult<Vec<User>> {
        let rows: Vec<UserRow> = sqlx::query_as(concat!(
            "SELECT ",
            user_columns!(),
            " FROM users WHERE id = ANY($1)"
        ))
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(User::try_from).collect()
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

#[derive(FromRow)]
struct CourseSearchRow {
    #[sqlx(flatten)]
    course: CourseRow,
    total: i64,
}

/// Escapes LIKE wildcards in user input.
fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
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

    async fn list_by_status(&self, status: CourseStatus) -> AppResult<Vec<Course>> {
        let rows: Vec<CourseRow> = sqlx::query_as(concat!(
            "SELECT ",
            course_columns!(),
            " FROM courses WHERE status = $1 ORDER BY updated_at"
        ))
        .bind(status.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Course::try_from).collect()
    }

    async fn search_published(&self, f: &CatalogFilter) -> AppResult<(Vec<Course>, i64)> {
        let q = f.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
        // Substring fallback (ILIKE) for words full-text search can't split, e.g. Khmer.
        let like = q.map(|q| format!("%{}%", escape_like(q)));
        let rows: Vec<CourseSearchRow> = sqlx::query_as(concat!(
            "SELECT ",
            course_columns!(),
            ", count(*) OVER () AS total
             FROM courses
             WHERE status = 'published'
               AND ($1::text IS NULL
                    OR search_vector @@ websearch_to_tsquery('simple', $1)
                    OR title ILIKE $2 OR subtitle ILIKE $2)
               AND ($3::uuid IS NULL OR category_id = $3)
               AND ($4::text IS NULL OR level = $4)
               AND ($5::text IS NULL OR ($5 = 'free' AND price_minor = 0)
                                     OR ($5 = 'paid' AND price_minor > 0))
             ORDER BY
               CASE WHEN $6 = 'relevance'
                    THEN ts_rank(search_vector, websearch_to_tsquery('simple', coalesce($1, '')))
               END DESC NULLS LAST,
               CASE WHEN $6 = 'price_asc'
                    THEN CASE currency WHEN 'KHR' THEN price_minor * 100.0 / $9 ELSE price_minor END
               END ASC,
               CASE WHEN $6 = 'price_desc'
                    THEN CASE currency WHEN 'KHR' THEN price_minor * 100.0 / $9 ELSE price_minor END
               END DESC,
               published_at DESC, id
             LIMIT $7 OFFSET $8"
        ))
        .bind(q)
        .bind(like)
        .bind(f.category_id)
        .bind(f.level.map(|l| l.as_str()))
        .bind(f.price.map(|p| match p {
            PriceFilter::Free => "free",
            PriceFilter::Paid => "paid",
        }))
        .bind(f.sort.as_str())
        .bind(f.limit)
        .bind(f.offset)
        .bind(KHR_PER_USD as f64)
        .fetch_all(&self.pool)
        .await?;
        let total = rows.first().map_or(0, |r| r.total);
        let courses = rows
            .into_iter()
            .map(|r| Course::try_from(r.course))
            .collect::<AppResult<_>>()?;
        Ok((courses, total))
    }

    async fn find_published_by_slug(&self, slug: &str) -> AppResult<Option<Course>> {
        let row: Option<CourseRow> = sqlx::query_as(concat!(
            "SELECT ",
            course_columns!(),
            " FROM courses WHERE slug = $1 AND status = 'published'"
        ))
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Course::try_from).transpose()
    }
}

#[derive(FromRow)]
struct SectionRow {
    id: Uuid,
    course_id: Uuid,
    title: String,
    position: i32,
}

impl From<SectionRow> for Section {
    fn from(r: SectionRow) -> Self {
        Section {
            id: r.id,
            course_id: r.course_id,
            title: r.title,
            position: r.position,
        }
    }
}

macro_rules! lecture_columns {
    () => {
        "id, course_id, section_id, title, kind, position, is_preview, article_body, duration_sec, \
         video_asset_id"
    };
}

#[derive(FromRow)]
struct LectureRow {
    id: Uuid,
    course_id: Uuid,
    section_id: Uuid,
    title: String,
    kind: String,
    position: i32,
    is_preview: bool,
    article_body: String,
    duration_sec: i32,
    video_asset_id: Option<Uuid>,
}

impl TryFrom<LectureRow> for Lecture {
    type Error = AppError;
    fn try_from(r: LectureRow) -> AppResult<Self> {
        Ok(Lecture {
            id: r.id,
            course_id: r.course_id,
            section_id: r.section_id,
            title: r.title,
            kind: parse_text::<LectureKind>(&r.kind)?,
            position: r.position,
            is_preview: r.is_preview,
            article_body: r.article_body,
            duration_sec: r.duration_sec,
            video_asset_id: r.video_asset_id,
        })
    }
}

#[async_trait]
impl CurriculumRepo for PgStore {
    async fn sections(&self, course_id: Uuid) -> AppResult<Vec<Section>> {
        let rows: Vec<SectionRow> = sqlx::query_as(
            "SELECT id, course_id, title, position FROM sections WHERE course_id = $1 ORDER BY position",
        )
        .bind(course_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn lectures(&self, course_id: Uuid) -> AppResult<Vec<Lecture>> {
        let rows: Vec<LectureRow> = sqlx::query_as(concat!(
            "SELECT ",
            lecture_columns!(),
            " FROM lectures WHERE course_id = $1 ORDER BY position"
        ))
        .bind(course_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Lecture::try_from).collect()
    }

    async fn find_section(&self, id: Uuid) -> AppResult<Option<Section>> {
        let row: Option<SectionRow> =
            sqlx::query_as("SELECT id, course_id, title, position FROM sections WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(Into::into))
    }

    async fn find_lecture(&self, id: Uuid) -> AppResult<Option<Lecture>> {
        let row: Option<LectureRow> = sqlx::query_as(concat!(
            "SELECT ",
            lecture_columns!(),
            " FROM lectures WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Lecture::try_from).transpose()
    }

    async fn insert_section(&self, s: &Section) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO sections (id, course_id, title, position) VALUES ($1, $2, $3, $4)",
        )
        .bind(s.id)
        .bind(s.course_id)
        .bind(&s.title)
        .bind(s.position)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn save_section(&self, s: &Section) -> AppResult<()> {
        sqlx::query("UPDATE sections SET title = $2, position = $3 WHERE id = $1")
            .bind(s.id)
            .bind(&s.title)
            .bind(s.position)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_section(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM sections WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn insert_lecture(&self, l: &Lecture) -> AppResult<()> {
        sqlx::query(concat!(
            "INSERT INTO lectures (",
            lecture_columns!(),
            ") VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
        ))
        .bind(l.id)
        .bind(l.course_id)
        .bind(l.section_id)
        .bind(&l.title)
        .bind(l.kind.as_str())
        .bind(l.position)
        .bind(l.is_preview)
        .bind(&l.article_body)
        .bind(l.duration_sec)
        .bind(l.video_asset_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn save_lecture(&self, l: &Lecture) -> AppResult<()> {
        sqlx::query(
            "UPDATE lectures SET section_id = $2, title = $3, position = $4, is_preview = $5,
                article_body = $6, duration_sec = $7, video_asset_id = $8
             WHERE id = $1",
        )
        .bind(l.id)
        .bind(l.section_id)
        .bind(&l.title)
        .bind(l.position)
        .bind(l.is_preview)
        .bind(&l.article_body)
        .bind(l.duration_sec)
        .bind(l.video_asset_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_lecture(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM lectures WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn reorder(&self, course_id: Uuid, order: &[SectionOrder]) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        for (s_pos, section) in order.iter().enumerate() {
            sqlx::query("UPDATE sections SET position = $3 WHERE id = $1 AND course_id = $2")
                .bind(section.section_id)
                .bind(course_id)
                .bind(s_pos as i32)
                .execute(&mut *tx)
                .await?;
            for (l_pos, lecture_id) in section.lecture_ids.iter().enumerate() {
                sqlx::query(
                    "UPDATE lectures SET section_id = $3, position = $4
                     WHERE id = $1 AND course_id = $2",
                )
                .bind(lecture_id)
                .bind(course_id)
                .bind(section.section_id)
                .bind(l_pos as i32)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    async fn stats(
        &self,
        course_ids: &[Uuid],
    ) -> AppResult<std::collections::HashMap<Uuid, LectureStats>> {
        let rows: Vec<(Uuid, i64, i64)> = sqlx::query_as(
            "SELECT course_id, count(*), coalesce(sum(duration_sec), 0)::bigint
             FROM lectures WHERE course_id = ANY($1) GROUP BY course_id",
        )
        .bind(course_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, lecture_count, duration_sec)| {
                (
                    id,
                    LectureStats {
                        lecture_count,
                        duration_sec,
                    },
                )
            })
            .collect())
    }
}

macro_rules! video_columns {
    () => {
        "id, owner_id, lecture_id, raw_key, status, duration_sec, error, playlists, attempts, \
         claimed_at, created_at, updated_at"
    };
}

#[derive(FromRow)]
struct VideoAssetRow {
    id: Uuid,
    owner_id: Uuid,
    lecture_id: Uuid,
    raw_key: String,
    status: String,
    duration_sec: i32,
    error: Option<String>,
    playlists: sqlx::types::Json<std::collections::BTreeMap<String, String>>,
    attempts: i32,
    claimed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<VideoAssetRow> for VideoAsset {
    type Error = AppError;
    fn try_from(r: VideoAssetRow) -> AppResult<Self> {
        Ok(VideoAsset {
            id: r.id,
            owner_id: r.owner_id,
            lecture_id: r.lecture_id,
            raw_key: r.raw_key,
            status: parse_text::<VideoStatus>(&r.status)?,
            duration_sec: r.duration_sec,
            error: r.error,
            playlists: r.playlists.0,
            attempts: r.attempts,
            claimed_at: r.claimed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

#[async_trait]
impl VideoAssetRepo for PgStore {
    async fn insert(&self, a: &VideoAsset) -> AppResult<()> {
        sqlx::query(concat!(
            "INSERT INTO video_assets (",
            video_columns!(),
            ") VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
        ))
        .bind(a.id)
        .bind(a.owner_id)
        .bind(a.lecture_id)
        .bind(&a.raw_key)
        .bind(a.status.as_str())
        .bind(a.duration_sec)
        .bind(&a.error)
        .bind(sqlx::types::Json(&a.playlists))
        .bind(a.attempts)
        .bind(a.claimed_at)
        .bind(a.created_at)
        .bind(a.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find(&self, id: Uuid) -> AppResult<Option<VideoAsset>> {
        let row: Option<VideoAssetRow> = sqlx::query_as(concat!(
            "SELECT ",
            video_columns!(),
            " FROM video_assets WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(VideoAsset::try_from).transpose()
    }

    async fn save(&self, a: &VideoAsset) -> AppResult<()> {
        sqlx::query(
            "UPDATE video_assets SET status = $2, duration_sec = $3, error = $4, playlists = $5,
                attempts = $6, claimed_at = $7, updated_at = $8
             WHERE id = $1",
        )
        .bind(a.id)
        .bind(a.status.as_str())
        .bind(a.duration_sec)
        .bind(&a.error)
        .bind(sqlx::types::Json(&a.playlists))
        .bind(a.attempts)
        .bind(a.claimed_at)
        .bind(a.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn claim_next(
        &self,
        now: DateTime<Utc>,
        stale_before: DateTime<Utc>,
    ) -> AppResult<Option<VideoAsset>> {
        // SKIP LOCKED lets several workers poll the same queue safely.
        let row: Option<VideoAssetRow> = sqlx::query_as(concat!(
            "UPDATE video_assets SET claimed_at = $1, attempts = attempts + 1
             WHERE id = (
                SELECT id FROM video_assets
                WHERE status = 'processing' AND (claimed_at IS NULL OR claimed_at < $2)
                ORDER BY created_at
                LIMIT 1
                FOR UPDATE SKIP LOCKED
             )
             RETURNING ",
            video_columns!()
        ))
        .bind(now)
        .bind(stale_before)
        .fetch_optional(&self.pool)
        .await?;
        row.map(VideoAsset::try_from).transpose()
    }
}
