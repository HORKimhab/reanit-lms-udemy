use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use super::model::PaymentAction;
use super::model::{Payment, text_enum};
use crate::{error::AppResult, modules::courses::model::Currency};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Bakong,
    AbaPayway,
}
text_enum!(ProviderKind { Bakong => "bakong", AbaPayway => "aba_payway" });

/// Everything a provider needs to start collecting one payment.
#[derive(Clone, Debug)]
pub struct PaymentRequest {
    pub payment_id: Uuid,
    pub order_id: Uuid,
    /// The payment method code, e.g. `aba_cards`.
    pub method: String,
    pub amount_minor: i64,
    pub currency: Currency,
    pub description: String,
    pub payer_name: String,
    pub payer_email: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct PaymentInit {
    pub reference: String,
    pub action: PaymentAction,
    pub raw: serde_json::Value,
}

/// The provider's view of a payment.
#[derive(Clone, Debug)]
pub enum ProviderStatus {
    Pending,
    Paid {
        provider_tran_id: String,
        amount_minor: i64,
        currency: Currency,
        raw: serde_json::Value,
    },
    Failed {
        reason: String,
        raw: serde_json::Value,
    },
}

#[async_trait]
pub trait PaymentProvider: Send + Sync {
    async fn create(&self, req: &PaymentRequest) -> AppResult<PaymentInit>;
    /// Asks the provider (never the client) whether the payment went through.
    async fn check(&self, payment: &Payment) -> AppResult<ProviderStatus>;
}

/// The configured providers; unconfigured ones are hidden from checkout.
pub trait PaymentProviders: Send + Sync {
    fn get(&self, kind: ProviderKind) -> Option<Arc<dyn PaymentProvider>>;
}

/// Providers fixed at startup from configuration.
#[derive(Default)]
pub struct ConfiguredProviders {
    pub bakong: Option<Arc<dyn PaymentProvider>>,
    pub aba_payway: Option<Arc<dyn PaymentProvider>>,
}

impl PaymentProviders for ConfiguredProviders {
    fn get(&self, kind: ProviderKind) -> Option<Arc<dyn PaymentProvider>> {
        match kind {
            ProviderKind::Bakong => self.bakong.clone(),
            ProviderKind::AbaPayway => self.aba_payway.clone(),
        }
    }
}
