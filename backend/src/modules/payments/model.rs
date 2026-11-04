use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::provider::ProviderKind;
use crate::modules::courses::model::Currency;

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }
        impl std::str::FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                match s {
                    $($text => Ok($name::$variant),)+
                    other => Err(format!("unknown {}: {other}", stringify!($name))),
                }
            }
        }
    };
}
pub(crate) use text_enum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    Pending,
    Paid,
    /// Left unpaid too long; check out again to buy.
    Expired,
}
text_enum!(OrderStatus { Pending => "pending", Paid => "paid", Expired => "expired" });

#[derive(Clone, Debug, Serialize)]
pub struct OrderItem {
    pub course_id: Uuid,
    /// Course title at the time of purchase.
    pub title: String,
    pub list_price_minor: i64,
    pub price_minor: i64,
    pub coupon_id: Option<Uuid>,
    pub coupon_code: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Order {
    pub id: Uuid,
    pub user_id: Uuid,
    pub status: OrderStatus,
    pub currency: Currency,
    pub subtotal_minor: i64,
    pub total_minor: i64,
    pub items: Vec<OrderItem>,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentStatus {
    Pending,
    Paid,
    Failed,
    Expired,
}
text_enum!(PaymentStatus { Pending => "pending", Paid => "paid", Failed => "failed", Expired => "expired" });

/// What the payer has to do next.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaymentAction {
    /// Scan with a banking app; `deeplink` opens the app directly on mobile.
    Qr {
        qr_string: String,
        deeplink: Option<String>,
    },
    /// POST these fields as an HTML form to `url` (hosted checkout).
    Redirect {
        url: String,
        fields: BTreeMap<String, String>,
    },
}

#[derive(Clone, Debug)]
pub struct Payment {
    pub id: Uuid,
    pub order_id: Uuid,
    /// `payment_methods.code`.
    pub method: String,
    pub provider: ProviderKind,
    pub amount_minor: i64,
    pub currency: Currency,
    pub status: PaymentStatus,
    /// How the provider identifies it: KHQR md5 (Bakong) or tran_id (PayWay).
    pub reference: String,
    pub provider_tran_id: Option<String>,
    pub action: PaymentAction,
    pub failure_reason: Option<String>,
    /// Latest provider response, kept for reconciliation.
    pub raw_response: Option<serde_json::Value>,
    pub expires_at: DateTime<Utc>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
}

/// An admin-managed payment option shown at checkout.
#[derive(Clone, Debug, Serialize)]
pub struct PaymentMethod {
    pub code: String,
    pub provider: ProviderKind,
    pub label: String,
    pub description: String,
    pub enabled: bool,
    pub currencies: Vec<Currency>,
    pub sort_order: i32,
}
