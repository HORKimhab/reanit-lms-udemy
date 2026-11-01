//! Postgres adapters for enrollments, carts, coupons, orders and payments.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::PgStore;
use crate::{
    error::AppResult,
    modules::{
        cart::{SavedCourseRepo, SavedList},
        enrollments::{Enrollment, EnrollmentRepo},
    },
};

#[derive(FromRow)]
struct EnrollmentRow {
    user_id: Uuid,
    course_id: Uuid,
    order_id: Option<Uuid>,
    enrolled_at: DateTime<Utc>,
}

impl From<EnrollmentRow> for Enrollment {
    fn from(r: EnrollmentRow) -> Self {
        Self {
            user_id: r.user_id,
            course_id: r.course_id,
            order_id: r.order_id,
            enrolled_at: r.enrolled_at,
        }
    }
}

#[async_trait]
impl EnrollmentRepo for PgStore {
    async fn enroll(&self, e: &Enrollment) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO enrollments (user_id, course_id, order_id, enrolled_at) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (user_id, course_id) DO NOTHING",
        )
        .bind(e.user_id)
        .bind(e.course_id)
        .bind(e.order_id)
        .bind(e.enrolled_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn is_enrolled(&self, user_id: Uuid, course_id: Uuid) -> AppResult<bool> {
        let found: Option<(i32,)> =
            sqlx::query_as("SELECT 1 FROM enrollments WHERE user_id = $1 AND course_id = $2")
                .bind(user_id)
                .bind(course_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(found.is_some())
    }

    async fn list_by_user(&self, user_id: Uuid) -> AppResult<Vec<Enrollment>> {
        let rows: Vec<EnrollmentRow> = sqlx::query_as(
            "SELECT user_id, course_id, order_id, enrolled_at FROM enrollments \
             WHERE user_id = $1 ORDER BY enrolled_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Enrollment::from).collect())
    }
}

#[async_trait]
impl SavedCourseRepo for PgStore {
    async fn add(
        &self,
        list: SavedList,
        user_id: Uuid,
        course_id: Uuid,
        at: DateTime<Utc>,
    ) -> AppResult<()> {
        let sql = match list {
            SavedList::Cart => {
                "INSERT INTO cart_items (user_id, course_id, added_at) VALUES ($1, $2, $3) \
                 ON CONFLICT DO NOTHING"
            }
            SavedList::Wishlist => {
                "INSERT INTO wishlist_items (user_id, course_id, added_at) VALUES ($1, $2, $3) \
                 ON CONFLICT DO NOTHING"
            }
        };
        sqlx::query(sql)
            .bind(user_id)
            .bind(course_id)
            .bind(at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn remove(&self, list: SavedList, user_id: Uuid, course_id: Uuid) -> AppResult<()> {
        let sql = match list {
            SavedList::Cart => "DELETE FROM cart_items WHERE user_id = $1 AND course_id = $2",
            SavedList::Wishlist => {
                "DELETE FROM wishlist_items WHERE user_id = $1 AND course_id = $2"
            }
        };
        sqlx::query(sql)
            .bind(user_id)
            .bind(course_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(&self, list: SavedList, user_id: Uuid) -> AppResult<Vec<Uuid>> {
        let sql = match list {
            SavedList::Cart => {
                "SELECT course_id FROM cart_items WHERE user_id = $1 ORDER BY added_at DESC"
            }
            SavedList::Wishlist => {
                "SELECT course_id FROM wishlist_items WHERE user_id = $1 ORDER BY added_at DESC"
            }
        };
        let rows: Vec<(Uuid,)> = sqlx::query_as(sql)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}
