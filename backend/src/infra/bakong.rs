//! Bakong (NBC) KHQR: we generate the dynamic KHQR ourselves and confirm
//! payment with the Bakong Open API's `check_transaction_by_md5`.

use async_trait::async_trait;
use serde_json::{Value, json};

use super::{
    http_json::{client, minor_units, post_json},
    khqr::{self, AccountKind, Khqr, Merchant},
};
use crate::{
    error::{AppError, AppResult},
    modules::payments::{
        model::{Payment, PaymentAction},
        provider::{PaymentInit, PaymentProvider, PaymentRequest, ProviderStatus},
    },
};

#[derive(Clone, Debug)]
pub struct BakongConfig {
    /// `https://api-bakong.nbc.gov.kh` (production) or the SIT host.
    pub api_url: String,
    /// Open API token from api-bakong.nbc.gov.kh (renewed every ~90 days).
    pub token: String,
    /// The receiving Bakong account, e.g. `reanit@abaa`.
    pub account_id: String,
    pub merchant_name: String,
    pub merchant_city: String,
    /// Set both for a merchant (tag 30) account; leave empty for individual (tag 29).
    pub merchant_id: Option<String>,
    pub acquiring_bank: Option<String>,
}

pub struct BakongProvider {
    config: BakongConfig,
    http: reqwest::Client,
}

impl BakongProvider {
    pub fn new(config: BakongConfig) -> Self {
        Self {
            config,
            http: client(),
        }
    }

    fn merchant(&self) -> Merchant {
        let c = &self.config;
        Merchant {
            bakong_account_id: c.account_id.clone(),
            name: c.merchant_name.clone(),
            city: c.merchant_city.clone(),
            kind: match (&c.merchant_id, &c.acquiring_bank) {
                (Some(id), Some(bank)) => AccountKind::Merchant {
                    merchant_id: id.clone(),
                    acquiring_bank: bank.clone(),
                },
                (_, bank) => AccountKind::Individual {
                    account_information: None,
                    acquiring_bank: bank.clone(),
                },
            },
        }
    }
}

#[async_trait]
impl PaymentProvider for BakongProvider {
    async fn create(&self, req: &PaymentRequest) -> AppResult<PaymentInit> {
        let bill_number: String = req.order_id.simple().to_string().chars().take(25).collect();
        let qr_string = khqr::encode(
            &self.merchant(),
            &Khqr {
                amount_minor: req.amount_minor,
                currency: req.currency,
                bill_number: Some(bill_number),
                store_label: None,
                terminal_label: None,
                created_at: req.created_at,
                expires_at: req.expires_at,
            },
        )
        .map_err(|e| AppError::Internal(anyhow::anyhow!("KHQR misconfigured: {e}")))?;
        let reference = khqr::md5_hex(&qr_string);
        Ok(PaymentInit {
            raw: json!({ "qr": qr_string, "md5": reference }),
            reference,
            action: PaymentAction::Qr {
                qr_string,
                deeplink: None,
            },
        })
    }

    async fn check(&self, payment: &Payment) -> AppResult<ProviderStatus> {
        let url = format!(
            "{}/v1/check_transaction_by_md5",
            self.config.api_url.trim_end_matches('/')
        );
        let (status, body) = post_json(
            &self.http,
            &url,
            Some(&self.config.token),
            &json!({ "md5": payment.reference }),
        )
        .await?;
        if !(200..300).contains(&status) {
            return Err(AppError::Upstream(format!("Bakong HTTP {status}: {body}")));
        }

        match (body["responseCode"].as_i64(), body["errorCode"].as_i64()) {
            (Some(0), _) => paid(&self.config.account_id, payment, body),
            // 1: not found yet (customer hasn't paid).
            (_, Some(1)) => Ok(ProviderStatus::Pending),
            // 3: transaction failed.
            (_, Some(3)) => Ok(ProviderStatus::Failed {
                reason: "the bank reported the transaction as failed".into(),
                raw: body,
            }),
            _ => Err(AppError::Upstream(format!("Bakong error: {body}"))),
        }
    }
}

fn paid(our_account: &str, payment: &Payment, body: Value) -> AppResult<ProviderStatus> {
    let data = &body["data"];
    let to = data["toAccountId"].as_str().unwrap_or_default();
    if !to.is_empty() && !to.eq_ignore_ascii_case(our_account) {
        return Ok(ProviderStatus::Failed {
            reason: format!("paid to {to}, not to {our_account}"),
            raw: body,
        });
    }
    let currency = data["currency"]
        .as_str()
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| AppError::Upstream(format!("Bakong: unexpected currency in {body}")))?;
    let amount_minor = minor_units(&data["amount"], currency)
        .ok_or_else(|| AppError::Upstream(format!("Bakong: unexpected amount in {body}")))?;
    Ok(ProviderStatus::Paid {
        provider_tran_id: data["hash"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| payment.reference.clone()),
        amount_minor,
        currency,
        raw: body,
    })
}
