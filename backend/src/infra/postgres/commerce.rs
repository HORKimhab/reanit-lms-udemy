//! Postgres adapters for enrollments, carts, coupons, orders and payments.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::PgStore;
use crate::{
    error::{AppError, AppResult},
    modules::{
        cart::{SavedCourseRepo, SavedList},
        coupons::{Coupon, CouponRepo},
        courses::model::Currency,
        enrollments::{Enrollment, EnrollmentRepo},
        payments::{
            model::{
                Order, OrderItem, OrderStatus, Payment, PaymentAction, PaymentMethod, PaymentStatus,
            },
            provider::ProviderKind,
            repo::{MethodPatch, OrderRepo, PaymentMethodRepo, Settlement},
        },
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

macro_rules! coupon_columns {
    () => {
        "id, course_id, code, discount_percent, expires_at, max_uses, used_count, created_at"
    };
}

#[derive(FromRow)]
struct CouponRow {
    id: Uuid,
    course_id: Uuid,
    code: String,
    discount_percent: i32,
    expires_at: Option<DateTime<Utc>>,
    max_uses: Option<i32>,
    used_count: i32,
    created_at: DateTime<Utc>,
}

impl From<CouponRow> for Coupon {
    fn from(r: CouponRow) -> Self {
        Self {
            id: r.id,
            course_id: r.course_id,
            code: r.code,
            discount_percent: r.discount_percent,
            expires_at: r.expires_at,
            max_uses: r.max_uses,
            used_count: r.used_count,
            created_at: r.created_at,
        }
    }
}

#[async_trait]
impl CouponRepo for PgStore {
    async fn insert(&self, c: &Coupon) -> AppResult<()> {
        sqlx::query(concat!(
            "INSERT INTO coupons (",
            coupon_columns!(),
            ") VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        ))
        .bind(c.id)
        .bind(c.course_id)
        .bind(&c.code)
        .bind(c.discount_percent)
        .bind(c.expires_at)
        .bind(c.max_uses)
        .bind(c.used_count)
        .bind(c.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                AppError::Conflict("this course already has a coupon with that code".into())
            }
            other => other.into(),
        })?;
        Ok(())
    }

    async fn list_by_course(&self, course_id: Uuid) -> AppResult<Vec<Coupon>> {
        let rows: Vec<CouponRow> = sqlx::query_as(concat!(
            "SELECT ",
            coupon_columns!(),
            " FROM coupons WHERE course_id = $1 ORDER BY created_at DESC"
        ))
        .bind(course_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Coupon::from).collect())
    }

    async fn find_for(&self, course_ids: &[Uuid], codes: &[String]) -> AppResult<Vec<Coupon>> {
        let rows: Vec<CouponRow> = sqlx::query_as(concat!(
            "SELECT ",
            coupon_columns!(),
            " FROM coupons WHERE course_id = ANY($1) AND code = ANY($2)"
        ))
        .bind(course_ids)
        .bind(codes)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Coupon::from).collect())
    }

    async fn delete(&self, course_id: Uuid, id: Uuid) -> AppResult<bool> {
        let done = sqlx::query("DELETE FROM coupons WHERE id = $1 AND course_id = $2")
            .bind(id)
            .bind(course_id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }
}

fn parse<T: std::str::FromStr<Err = String>>(s: &str) -> AppResult<T> {
    s.parse()
        .map_err(|e: String| AppError::Internal(anyhow::anyhow!(e)))
}

#[derive(FromRow)]
struct OrderRow {
    id: Uuid,
    user_id: Uuid,
    status: String,
    currency: String,
    subtotal_minor: i64,
    total_minor: i64,
    created_at: DateTime<Utc>,
    paid_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct OrderItemRow {
    order_id: Uuid,
    course_id: Uuid,
    title: String,
    list_price_minor: i64,
    price_minor: i64,
    coupon_id: Option<Uuid>,
    coupon_code: Option<String>,
}

macro_rules! order_columns {
    () => {
        "id, user_id, status, currency, subtotal_minor, total_minor, created_at, paid_at"
    };
}

macro_rules! payment_columns {
    () => {
        "id, order_id, method, provider, amount_minor, currency, status, reference, \
         provider_tran_id, action, failure_reason, raw_response, expires_at, last_checked_at, \
         created_at, paid_at"
    };
}

#[derive(FromRow)]
struct PaymentRow {
    id: Uuid,
    order_id: Uuid,
    method: String,
    provider: String,
    amount_minor: i64,
    currency: String,
    status: String,
    reference: String,
    provider_tran_id: Option<String>,
    action: sqlx::types::Json<PaymentAction>,
    failure_reason: Option<String>,
    raw_response: Option<serde_json::Value>,
    expires_at: DateTime<Utc>,
    last_checked_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    paid_at: Option<DateTime<Utc>>,
}

impl TryFrom<PaymentRow> for Payment {
    type Error = AppError;
    fn try_from(r: PaymentRow) -> AppResult<Self> {
        Ok(Self {
            id: r.id,
            order_id: r.order_id,
            method: r.method,
            provider: parse(&r.provider)?,
            amount_minor: r.amount_minor,
            currency: parse(&r.currency)?,
            status: parse(&r.status)?,
            reference: r.reference,
            provider_tran_id: r.provider_tran_id,
            action: r.action.0,
            failure_reason: r.failure_reason,
            raw_response: r.raw_response,
            expires_at: r.expires_at,
            last_checked_at: r.last_checked_at,
            created_at: r.created_at,
            paid_at: r.paid_at,
        })
    }
}

impl PgStore {
    async fn orders_with_items(&self, rows: Vec<OrderRow>) -> AppResult<Vec<Order>> {
        let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
        let items: Vec<OrderItemRow> = sqlx::query_as(
            "SELECT order_id, course_id, title, list_price_minor, price_minor, coupon_id, coupon_code \
             FROM order_items WHERE order_id = ANY($1) ORDER BY title",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(Order {
                    items: items
                        .iter()
                        .filter(|i| i.order_id == r.id)
                        .map(|i| OrderItem {
                            course_id: i.course_id,
                            title: i.title.clone(),
                            list_price_minor: i.list_price_minor,
                            price_minor: i.price_minor,
                            coupon_id: i.coupon_id,
                            coupon_code: i.coupon_code.clone(),
                        })
                        .collect(),
                    id: r.id,
                    user_id: r.user_id,
                    status: parse::<OrderStatus>(&r.status)?,
                    currency: parse::<Currency>(&r.currency)?,
                    subtotal_minor: r.subtotal_minor,
                    total_minor: r.total_minor,
                    created_at: r.created_at,
                    paid_at: r.paid_at,
                })
            })
            .collect()
    }
}

#[async_trait]
impl OrderRepo for PgStore {
    async fn insert_order(&self, o: &Order) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(concat!(
            "INSERT INTO orders (",
            order_columns!(),
            ") VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        ))
        .bind(o.id)
        .bind(o.user_id)
        .bind(o.status.as_str())
        .bind(o.currency.as_str())
        .bind(o.subtotal_minor)
        .bind(o.total_minor)
        .bind(o.created_at)
        .bind(o.paid_at)
        .execute(&mut *tx)
        .await?;
        for i in &o.items {
            sqlx::query(
                "INSERT INTO order_items (order_id, course_id, title, list_price_minor, price_minor, \
                 coupon_id, coupon_code) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(o.id)
            .bind(i.course_id)
            .bind(&i.title)
            .bind(i.list_price_minor)
            .bind(i.price_minor)
            .bind(i.coupon_id)
            .bind(&i.coupon_code)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn find_order(&self, id: Uuid) -> AppResult<Option<Order>> {
        let rows: Vec<OrderRow> = sqlx::query_as(concat!(
            "SELECT ",
            order_columns!(),
            " FROM orders WHERE id = $1"
        ))
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        Ok(self.orders_with_items(rows).await?.into_iter().next())
    }

    async fn list_orders(&self, user_id: Uuid) -> AppResult<Vec<Order>> {
        let rows: Vec<OrderRow> = sqlx::query_as(concat!(
            "SELECT ",
            order_columns!(),
            " FROM orders WHERE user_id = $1 ORDER BY created_at DESC"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        self.orders_with_items(rows).await
    }

    async fn complete_order(
        &self,
        order_id: Uuid,
        settlement: Option<&Settlement>,
        at: DateTime<Utc>,
    ) -> AppResult<bool> {
        let mut tx = self.pool.begin().await?;
        if let Some(s) = settlement {
            sqlx::query(
                "UPDATE payments SET status = 'paid', provider_tran_id = $2, raw_response = $3, \
                 paid_at = $4 WHERE id = $1 AND status = 'pending'",
            )
            .bind(s.payment_id)
            .bind(&s.provider_tran_id)
            .bind(&s.raw)
            .bind(at)
            .execute(&mut *tx)
            .await?;
        }
        let buyer: Option<(Uuid,)> = sqlx::query_as(
            "UPDATE orders SET status = 'paid', paid_at = $2 \
             WHERE id = $1 AND status = 'pending' RETURNING user_id",
        )
        .bind(order_id)
        .bind(at)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((user_id,)) = buyer else {
            tx.commit().await?;
            return Ok(false);
        };
        sqlx::query(
            "INSERT INTO enrollments (user_id, course_id, order_id, enrolled_at) \
             SELECT $1, course_id, order_id, $3 FROM order_items WHERE order_id = $2 \
             ON CONFLICT (user_id, course_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(order_id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM cart_items WHERE user_id = $1 \
             AND course_id IN (SELECT course_id FROM order_items WHERE order_id = $2)",
        )
        .bind(user_id)
        .bind(order_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE coupons SET used_count = used_count + 1 \
             WHERE id IN (SELECT coupon_id FROM order_items WHERE order_id = $1)",
        )
        .bind(order_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    async fn insert_payment(&self, p: &Payment) -> AppResult<()> {
        sqlx::query(concat!(
            "INSERT INTO payments (",
            payment_columns!(),
            ") VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)"
        ))
        .bind(p.id)
        .bind(p.order_id)
        .bind(&p.method)
        .bind(p.provider.as_str())
        .bind(p.amount_minor)
        .bind(p.currency.as_str())
        .bind(p.status.as_str())
        .bind(&p.reference)
        .bind(&p.provider_tran_id)
        .bind(sqlx::types::Json(&p.action))
        .bind(&p.failure_reason)
        .bind(&p.raw_response)
        .bind(p.expires_at)
        .bind(p.last_checked_at)
        .bind(p.created_at)
        .bind(p.paid_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_payment_by_reference(
        &self,
        provider: ProviderKind,
        reference: &str,
    ) -> AppResult<Option<Payment>> {
        let row: Option<PaymentRow> = sqlx::query_as(concat!(
            "SELECT ",
            payment_columns!(),
            " FROM payments WHERE provider = $1 AND reference = $2"
        ))
        .bind(provider.as_str())
        .bind(reference)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Payment::try_from).transpose()
    }

    async fn list_payments(&self, order_id: Uuid) -> AppResult<Vec<Payment>> {
        let rows: Vec<PaymentRow> = sqlx::query_as(concat!(
            "SELECT ",
            payment_columns!(),
            " FROM payments WHERE order_id = $1 ORDER BY created_at DESC, id"
        ))
        .bind(order_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Payment::try_from).collect()
    }

    async fn list_pending_payments(&self, limit: i64) -> AppResult<Vec<Payment>> {
        let rows: Vec<PaymentRow> = sqlx::query_as(concat!(
            "SELECT ",
            payment_columns!(),
            " FROM payments WHERE status = 'pending' ORDER BY created_at LIMIT $1"
        ))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Payment::try_from).collect()
    }

    async fn close_payment(
        &self,
        id: Uuid,
        status: PaymentStatus,
        reason: Option<&str>,
        raw: Option<&serde_json::Value>,
    ) -> AppResult<()> {
        sqlx::query(
            "UPDATE payments SET status = $2, failure_reason = $3, \
             raw_response = COALESCE($4, raw_response) WHERE id = $1 AND status = 'pending'",
        )
        .bind(id)
        .bind(status.as_str())
        .bind(reason)
        .bind(raw)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn touch_payment(&self, id: Uuid, checked_at: DateTime<Utc>) -> AppResult<()> {
        sqlx::query("UPDATE payments SET last_checked_at = $2 WHERE id = $1")
            .bind(id)
            .bind(checked_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[derive(FromRow)]
struct MethodRow {
    code: String,
    provider: String,
    label: String,
    description: String,
    enabled: bool,
    currencies: Vec<String>,
    sort_order: i32,
}

impl TryFrom<MethodRow> for PaymentMethod {
    type Error = AppError;
    fn try_from(r: MethodRow) -> AppResult<Self> {
        Ok(Self {
            provider: parse(&r.provider)?,
            currencies: r
                .currencies
                .iter()
                .map(|c| parse(c))
                .collect::<AppResult<_>>()?,
            code: r.code,
            label: r.label,
            description: r.description,
            enabled: r.enabled,
            sort_order: r.sort_order,
        })
    }
}

#[async_trait]
impl PaymentMethodRepo for PgStore {
    async fn list_methods(&self) -> AppResult<Vec<PaymentMethod>> {
        let rows: Vec<MethodRow> = sqlx::query_as(
            "SELECT code, provider, label, description, enabled, currencies, sort_order \
             FROM payment_methods ORDER BY sort_order, code",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(PaymentMethod::try_from).collect()
    }

    async fn update_method(&self, code: &str, p: &MethodPatch) -> AppResult<Option<PaymentMethod>> {
        let currencies: Option<Vec<&str>> = p
            .currencies
            .as_ref()
            .map(|cs| cs.iter().map(|c| c.as_str()).collect());
        let row: Option<MethodRow> = sqlx::query_as(
            "UPDATE payment_methods SET \
               enabled = COALESCE($2, enabled), \
               label = COALESCE($3, label), \
               description = COALESCE($4, description), \
               sort_order = COALESCE($5, sort_order), \
               currencies = COALESCE($6, currencies) \
             WHERE code = $1 \
             RETURNING code, provider, label, description, enabled, currencies, sort_order",
        )
        .bind(code)
        .bind(p.enabled)
        .bind(&p.label)
        .bind(&p.description)
        .bind(p.sort_order)
        .bind(currencies)
        .fetch_optional(&self.pool)
        .await?;
        row.map(PaymentMethod::try_from).transpose()
    }
}
