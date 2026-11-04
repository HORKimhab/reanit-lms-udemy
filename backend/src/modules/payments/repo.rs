use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{
    model::{Order, Payment, PaymentMethod, PaymentStatus},
    provider::ProviderKind,
};
use crate::error::AppResult;

/// A provider-confirmed payment, ready to be recorded.
#[derive(Clone, Debug)]
pub struct Settlement {
    pub payment_id: Uuid,
    pub provider_tran_id: String,
    pub raw: serde_json::Value,
}

#[async_trait]
pub trait OrderRepo: Send + Sync {
    async fn insert_order(&self, order: &Order) -> AppResult<()>;
    async fn find_order(&self, id: Uuid) -> AppResult<Option<Order>>;
    /// Newest first.
    async fn list_orders(&self, user_id: Uuid) -> AppResult<Vec<Order>>;

    /// Atomically: marks the payment (if any) and the order paid, enrolls the
    /// buyer in every item, removes them from the cart and counts coupon uses.
    /// Returns false (changing nothing for the order) if it was already paid.
    async fn complete_order(
        &self,
        order_id: Uuid,
        settlement: Option<&Settlement>,
        at: DateTime<Utc>,
    ) -> AppResult<bool>;

    /// Expires pending orders created before `created_before` that have no
    /// pending payment. Returns how many were expired.
    async fn expire_stale_orders(&self, created_before: DateTime<Utc>) -> AppResult<u64>;

    async fn insert_payment(&self, payment: &Payment) -> AppResult<()>;
    async fn find_payment_by_reference(
        &self,
        provider: ProviderKind,
        reference: &str,
    ) -> AppResult<Option<Payment>>;
    /// Most recent first.
    async fn list_payments(&self, order_id: Uuid) -> AppResult<Vec<Payment>>;
    /// Pending payments, oldest first (for the background checker).
    async fn list_pending_payments(&self, limit: i64) -> AppResult<Vec<Payment>>;
    /// Moves a still-pending payment to `failed`/`expired`; no-op otherwise.
    async fn close_payment(
        &self,
        id: Uuid,
        status: PaymentStatus,
        reason: Option<&str>,
        raw: Option<&serde_json::Value>,
    ) -> AppResult<()>;
    async fn touch_payment(&self, id: Uuid, checked_at: DateTime<Utc>) -> AppResult<()>;
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct MethodPatch {
    pub enabled: Option<bool>,
    pub label: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
    pub currencies: Option<Vec<crate::modules::courses::model::Currency>>,
}

#[async_trait]
pub trait PaymentMethodRepo: Send + Sync {
    /// All methods by sort order, then code.
    async fn list_methods(&self) -> AppResult<Vec<PaymentMethod>>;
    async fn update_method(
        &self,
        code: &str,
        patch: &MethodPatch,
    ) -> AppResult<Option<PaymentMethod>>;
}
