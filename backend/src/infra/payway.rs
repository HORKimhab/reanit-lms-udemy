//! ABA PayWay: the QR API for ABA KHQR / ABA Mobile deeplinks, the hosted
//! Purchase page for cards, and Check Transaction to confirm payments.
//! Requests are signed with base64(HMAC-SHA512(concatenated fields, api key)).

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::Sha512;

use super::{
    clock::Clock,
    http_json::{client, minor_units, post_json},
};
use crate::{
    error::{AppError, AppResult},
    modules::{
        courses::model::Currency,
        payments::{
            model::{Payment, PaymentAction},
            provider::{PaymentInit, PaymentProvider, PaymentRequest, ProviderStatus},
        },
    },
};

const QR_PATH: &str = "/api/payment-gateway/v1/payments/generate-qr";
const PURCHASE_PATH: &str = "/api/payment-gateway/v1/payments/purchase";
const CHECK_PATH: &str = "/api/payment-gateway/v1/payments/check-transaction-2";

#[derive(Clone, Debug)]
pub struct PaywayConfig {
    /// `https://checkout-sandbox.payway.com.kh` or the production host.
    pub base_url: String,
    pub merchant_id: String,
    /// The "public key" PayWay issues; used as the HMAC key.
    pub api_key: String,
    /// Our public pushback endpoint (must be whitelisted with PayWay).
    pub callback_url: String,
    /// Where card payers return to: `{frontend_url}/checkout/{order_id}`.
    pub frontend_url: String,
}

pub struct PaywayProvider {
    config: PaywayConfig,
    clock: Arc<dyn Clock>,
    http: reqwest::Client,
}

/// PayWay's `items` field: base64 of a JSON array (informational only).
#[derive(Serialize)]
struct Item<'a> {
    name: &'a str,
    quantity: u32,
    price: serde_json::Number,
}

/// The amount as both the JSON number sent and the exact text that is hashed
/// (`20`, `19.99`, `40000`), so the two can never disagree.
fn amount(amount_minor: i64, currency: Currency) -> (String, serde_json::Number) {
    match currency {
        Currency::Usd if amount_minor % 100 != 0 => {
            let n =
                serde_json::Number::from_f64(amount_minor as f64 / 100.0).expect("finite amount");
            (n.to_string(), n)
        }
        Currency::Usd => {
            let n = serde_json::Number::from(amount_minor / 100);
            (n.to_string(), n)
        }
        Currency::Khr => {
            let n = serde_json::Number::from(amount_minor);
            (n.to_string(), n)
        }
    }
}

impl PaywayProvider {
    pub fn new(config: PaywayConfig, clock: Arc<dyn Clock>) -> Self {
        Self {
            config,
            clock,
            http: client(),
        }
    }

    fn sign(&self, parts: &[&str]) -> String {
        let mut mac = Hmac::<Sha512>::new_from_slice(self.config.api_key.as_bytes())
            .expect("HMAC takes any key length");
        for part in parts {
            mac.update(part.as_bytes());
        }
        B64.encode(mac.finalize().into_bytes())
    }

    fn req_time(&self) -> String {
        self.clock.now().format("%Y%m%d%H%M%S").to_string()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.base_url.trim_end_matches('/'))
    }

    async fn create_qr(
        &self,
        req: &PaymentRequest,
        tran_id: &str,
        items: &str,
    ) -> AppResult<PaymentInit> {
        let req_time = self.req_time();
        let (amount_text, amount_json) = amount(req.amount_minor, req.currency);
        let callback_url = B64.encode(&self.config.callback_url);
        let lifetime = lifetime_minutes(req).to_string();
        let (purchase_type, payment_option, template) =
            ("purchase", "abapay_khqr", "template3_color");
        let hash = self.sign(&[
            &req_time,
            &self.config.merchant_id,
            tran_id,
            &amount_text,
            items,
            "", // first_name
            "", // last_name
            &req.payer_email,
            "", // phone
            purchase_type,
            payment_option,
            &callback_url,
            "", // return_deeplink
            req.currency.as_str(),
            "", // custom_fields
            "", // return_params
            "", // payout
            &lifetime,
            template,
        ]);
        let body = json!({
            "req_time": req_time,
            "merchant_id": self.config.merchant_id,
            "tran_id": tran_id,
            "email": req.payer_email,
            "amount": amount_json,
            "currency": req.currency.as_str(),
            "purchase_type": purchase_type,
            "payment_option": payment_option,
            "items": items,
            "callback_url": callback_url,
            "lifetime": lifetime_minutes(req),
            "qr_image_template": template,
            "hash": hash,
        });
        let (status, res) = post_json(&self.http, &self.url(QR_PATH), None, &body).await?;
        if status_code(&res) != Some(0) || !(200..300).contains(&status) {
            return Err(AppError::Upstream(format!(
                "PayWay QR HTTP {status}: {res}"
            )));
        }
        let qr_string = res["qrString"]
            .as_str()
            .ok_or_else(|| AppError::Upstream(format!("PayWay QR: no qrString in {res}")))?
            .to_string();
        let deeplink = res["abapay_deeplink"].as_str().map(String::from);
        let mut raw = res;
        // The image is large and we render our own.
        if let Some(obj) = raw.as_object_mut() {
            obj.remove("qrImage");
        }
        Ok(PaymentInit {
            reference: tran_id.to_string(),
            action: PaymentAction::Qr {
                qr_string,
                deeplink,
            },
            raw,
        })
    }

    fn create_card_checkout(
        &self,
        req: &PaymentRequest,
        tran_id: &str,
        items: &str,
    ) -> PaymentInit {
        let req_time = self.req_time();
        let (amount_text, _) = amount(req.amount_minor, req.currency);
        let return_url = B64.encode(&self.config.callback_url);
        let back = format!(
            "{}/checkout/{}",
            self.config.frontend_url.trim_end_matches('/'),
            req.order_id
        );
        let lifetime = lifetime_minutes(req).to_string();
        let hash = self.sign(&[
            &req_time,
            &self.config.merchant_id,
            tran_id,
            &amount_text,
            items,
            "", // shipping
            "", // firstname
            "", // lastname
            &req.payer_email,
            "", // phone
            "purchase",
            "cards",
            &return_url,
            &back, // cancel_url
            &back, // continue_success_url
            "",    // return_deeplink
            req.currency.as_str(),
            "", // custom_fields
            "", // return_params
            "", // payout
            &lifetime,
            "", // additional_params
            "", // google_pay_token
            "", // skip_success_page
        ]);
        let fields: BTreeMap<String, String> = [
            ("req_time", req_time),
            ("merchant_id", self.config.merchant_id.clone()),
            ("tran_id", tran_id.to_string()),
            ("amount", amount_text),
            ("items", items.to_string()),
            ("email", req.payer_email.clone()),
            ("type", "purchase".into()),
            ("payment_option", "cards".into()),
            ("return_url", return_url),
            ("cancel_url", back.clone()),
            ("continue_success_url", back),
            ("currency", req.currency.as_str().into()),
            ("lifetime", lifetime),
            ("hash", hash),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        PaymentInit {
            reference: tran_id.to_string(),
            raw: json!({ "form": fields }),
            action: PaymentAction::Redirect {
                url: self.url(PURCHASE_PATH),
                fields,
            },
        }
    }
}

/// QR API minimum is 3 minutes.
fn lifetime_minutes(req: &PaymentRequest) -> i64 {
    (req.expires_at - req.created_at).num_minutes().max(3)
}

/// `status.code` may be a string ("00") or a number; it sits at the top level
/// or inside `data` depending on the endpoint.
fn status_code(res: &Value) -> Option<i64> {
    let code = if res["status"]["code"].is_null() {
        &res["data"]["status"]["code"]
    } else {
        &res["status"]["code"]
    };
    match code {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[async_trait]
impl PaymentProvider for PaywayProvider {
    async fn create(&self, req: &PaymentRequest) -> AppResult<PaymentInit> {
        // tran_id: max 20 characters, unique per merchant.
        let tran_id: String = req
            .payment_id
            .simple()
            .to_string()
            .chars()
            .take(20)
            .collect();
        let (_, price) = amount(req.amount_minor, req.currency);
        let name: String = req.description.chars().take(100).collect();
        let items_json = serde_json::to_vec(&[Item {
            name: &name,
            quantity: 1,
            price,
        }])
        .map_err(anyhow::Error::from)?;
        let items = B64.encode(items_json);
        match req.method.as_str() {
            "aba_cards" => Ok(self.create_card_checkout(req, &tran_id, &items)),
            _ => self.create_qr(req, &tran_id, &items).await,
        }
    }

    async fn check(&self, payment: &Payment) -> AppResult<ProviderStatus> {
        let req_time = self.req_time();
        let hash = self.sign(&[&req_time, &self.config.merchant_id, &payment.reference]);
        let body = json!({
            "req_time": req_time,
            "merchant_id": self.config.merchant_id,
            "tran_id": payment.reference,
            "hash": hash,
        });
        let (status, res) = post_json(&self.http, &self.url(CHECK_PATH), None, &body).await?;
        if !(200..300).contains(&status) {
            return Err(AppError::Upstream(format!(
                "PayWay check HTTP {status}: {res}"
            )));
        }
        match status_code(&res) {
            Some(0) => {}
            // 6: transaction not found (not paid / not created yet).
            Some(6) => return Ok(ProviderStatus::Pending),
            _ => return Err(AppError::Upstream(format!("PayWay check: {res}"))),
        }
        let data = &res["data"];
        match data["payment_status"].as_str().unwrap_or_default() {
            "APPROVED" => {
                // The transaction is in our order's currency; total_amount is
                // what the payer had to pay for it.
                let amount_minor = minor_units(&data["total_amount"], payment.currency)
                    .ok_or_else(|| AppError::Upstream(format!("PayWay: no amount in {res}")))?;
                Ok(ProviderStatus::Paid {
                    provider_tran_id: payment.reference.clone(),
                    amount_minor,
                    currency: payment.currency,
                    raw: res,
                })
            }
            "PENDING" | "PRE-AUTH" => Ok(ProviderStatus::Pending),
            other => Ok(ProviderStatus::Failed {
                reason: format!(
                    "PayWay status {}",
                    if other.is_empty() { "unknown" } else { other }
                ),
                raw: res,
            }),
        }
    }
}
