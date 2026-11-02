//! The real Bakong / PayWay adapters against a local stub of each API.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    body::Bytes,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use chrono::{TimeZone, Utc};
use reanit_api::{
    infra::{
        bakong::{BakongConfig, BakongProvider},
        clock::Clock,
        khqr,
        payway::{PaywayConfig, PaywayProvider},
    },
    modules::{
        courses::model::Currency,
        payments::{
            model::{Payment, PaymentAction, PaymentStatus},
            provider::{PaymentProvider, PaymentRequest, ProviderKind, ProviderStatus},
        },
    },
};
use serde_json::{Value, json};
use uuid::Uuid;

/// What the stub saw: (path, headers, body).
type Seen = Arc<Mutex<Vec<(String, HeaderMap, Bytes)>>>;

/// Serves `responses[path]` for POSTs and records every request.
async fn stub(responses: Vec<(&'static str, StatusCode, Value)>) -> (String, Seen) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let seen: Seen = Arc::default();
    let mut router = Router::new();
    for (path, status, body) in responses {
        let seen = seen.clone();
        router = router.route(
            path,
            post(move |headers: HeaderMap, bytes: Bytes| {
                let seen = seen.clone();
                let body = body.clone();
                async move {
                    seen.lock()
                        .unwrap()
                        .push((path.to_string(), headers, bytes));
                    (status, Json(body)).into_response()
                }
            }),
        );
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (format!("http://{addr}"), seen)
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 12, 9, 54, 39).unwrap()
    }
}

fn request(method: &str, amount_minor: i64, currency: Currency) -> PaymentRequest {
    let now = FixedClock.now();
    PaymentRequest {
        payment_id: Uuid::parse_str("0123456789abcdef0123456789abcdef").unwrap(),
        order_id: Uuid::parse_str("fedcba9876543210fedcba9876543210").unwrap(),
        method: method.into(),
        amount_minor,
        currency,
        description: "Rust Basics".into(),
        payer_name: "Dara Sok".into(),
        payer_email: "dara@example.com".into(),
        created_at: now,
        expires_at: now + chrono::Duration::minutes(10),
    }
}

fn payment(
    provider: ProviderKind,
    reference: &str,
    amount_minor: i64,
    currency: Currency,
) -> Payment {
    let now = FixedClock.now();
    Payment {
        id: Uuid::new_v4(),
        order_id: Uuid::new_v4(),
        method: "x".into(),
        provider,
        amount_minor,
        currency,
        status: PaymentStatus::Pending,
        reference: reference.into(),
        provider_tran_id: None,
        action: PaymentAction::Qr {
            qr_string: String::new(),
            deeplink: None,
        },
        failure_reason: None,
        raw_response: None,
        expires_at: now,
        last_checked_at: None,
        created_at: now,
        paid_at: None,
    }
}

fn json_body(seen: &Seen, i: usize) -> Value {
    serde_json::from_slice(&seen.lock().unwrap()[i].2).unwrap()
}

// ---------- Bakong ----------

fn bakong(api_url: &str) -> BakongProvider {
    BakongProvider::new(BakongConfig {
        api_url: api_url.into(),
        token: "bakong-token".into(),
        account_id: "reanit@abaa".into(),
        merchant_name: "ReanIT".into(),
        merchant_city: "Phnom Penh".into(),
        merchant_id: None,
        acquiring_bank: None,
    })
}

#[tokio::test]
async fn bakong_creates_a_dynamic_khqr_referenced_by_its_md5() {
    let provider = bakong("http://unused.test");

    let init = provider
        .create(&request("bakong_khqr", 1999, Currency::Usd))
        .await
        .unwrap();

    let PaymentAction::Qr {
        qr_string,
        deeplink,
    } = init.action
    else {
        panic!("expected a QR")
    };
    assert_eq!(deeplink, None);
    assert!(qr_string.starts_with("000201010212"), "{qr_string}");
    assert!(qr_string.contains("0011reanit@abaa"), "{qr_string}");
    assert!(qr_string.contains("5303840540519.99"), "{qr_string}");
    assert!(qr_string.contains("5906ReanIT"), "{qr_string}");
    assert_eq!(
        khqr::crc16(&qr_string[..qr_string.len() - 4]),
        qr_string[qr_string.len() - 4..]
    );
    assert_eq!(init.reference, khqr::md5_hex(&qr_string));
}

#[tokio::test]
async fn bakong_reports_a_found_transaction_as_paid() {
    let (url, seen) = stub(vec![(
        "/v1/check_transaction_by_md5",
        StatusCode::OK,
        json!({
            "responseCode": 0,
            "responseMessage": "Getting transaction successfully.",
            "errorCode": null,
            "data": {
                "hash": "8465d722d7d5065f2886ab2a5b2a9b8b9a2b8b",
                "fromAccountId": "buyer@aclb",
                "toAccountId": "reanit@abaa",
                "currency": "USD",
                "amount": 19.99,
                "description": "",
                "createdDateMs": 1741773279000_i64,
                "acknowledgedDateMs": 1741773280000_i64
            }
        }),
    )])
    .await;

    let status = bakong(&url)
        .check(&payment(
            ProviderKind::Bakong,
            "abc123md5",
            1999,
            Currency::Usd,
        ))
        .await
        .unwrap();

    match status {
        ProviderStatus::Paid {
            provider_tran_id,
            amount_minor,
            currency,
            ..
        } => {
            assert_eq!(provider_tran_id, "8465d722d7d5065f2886ab2a5b2a9b8b9a2b8b");
            assert_eq!(amount_minor, 1999);
            assert_eq!(currency, Currency::Usd);
        }
        other => panic!("expected paid, got {other:?}"),
    }
    let headers = seen.lock().unwrap()[0].1.clone();
    assert_eq!(headers["authorization"], "Bearer bakong-token");
    assert_eq!(json_body(&seen, 0), json!({ "md5": "abc123md5" }));
}

#[tokio::test]
async fn bakong_not_found_is_still_pending_and_failed_is_failed() {
    let (url, _) = stub(vec![(
        "/v1/check_transaction_by_md5",
        StatusCode::OK,
        json!({ "responseCode": 1, "responseMessage": "Transaction could not be found.", "errorCode": 1, "data": null }),
    )])
    .await;
    let pending = bakong(&url)
        .check(&payment(ProviderKind::Bakong, "m", 100, Currency::Usd))
        .await
        .unwrap();
    assert!(matches!(pending, ProviderStatus::Pending));

    let (url, _) = stub(vec![(
        "/v1/check_transaction_by_md5",
        StatusCode::OK,
        json!({ "responseCode": 1, "responseMessage": "Transaction failed.", "errorCode": 3, "data": null }),
    )])
    .await;
    let failed = bakong(&url)
        .check(&payment(ProviderKind::Bakong, "m", 100, Currency::Usd))
        .await
        .unwrap();
    assert!(matches!(failed, ProviderStatus::Failed { .. }));
}

#[tokio::test]
async fn bakong_payments_to_another_account_are_not_accepted() {
    let (url, _) = stub(vec![(
        "/v1/check_transaction_by_md5",
        StatusCode::OK,
        json!({ "responseCode": 0, "errorCode": null, "data": {
            "hash": "h", "toAccountId": "someone@else", "currency": "KHR", "amount": 40000
        }}),
    )])
    .await;

    let status = bakong(&url)
        .check(&payment(ProviderKind::Bakong, "m", 40000, Currency::Khr))
        .await
        .unwrap();

    assert!(
        matches!(status, ProviderStatus::Failed { .. }),
        "{status:?}"
    );
}

#[tokio::test]
async fn bakong_auth_errors_surface_as_errors() {
    let (url, _) = stub(vec![(
        "/v1/check_transaction_by_md5",
        StatusCode::UNAUTHORIZED,
        json!({ "responseCode": 1, "errorCode": 6, "responseMessage": "Unauthorized" }),
    )])
    .await;

    let result = bakong(&url)
        .check(&payment(ProviderKind::Bakong, "m", 100, Currency::Usd))
        .await;

    assert!(result.is_err());
}

// ---------- ABA PayWay ----------

fn payway(base_url: &str) -> PaywayProvider {
    PaywayProvider::new(
        PaywayConfig {
            base_url: base_url.into(),
            merchant_id: "ec000002".into(),
            api_key: "test-api-key".into(),
            callback_url: "https://api.reanit.test/api/v1/webhooks/aba-payway".into(),
            frontend_url: "https://reanit.test".into(),
        },
        Arc::new(FixedClock),
    )
}

const QR_RESPONSE: &str = r#"{
    "qrString": "00020101021230510016abaakhppxxx@abaa0115125021214532846",
    "qrImage": "data:image/png;base64,iVBORw0KGgo=",
    "abapay_deeplink": "abamobilebank://ababank.com?type=payway&qrcode=000201",
    "app_store": "https://itunes.apple.com/al/app/aba-mobile-bank/id968860649?mt=8",
    "play_store": "https://play.google.com/store/apps/details?id=com.paygo24.ibank",
    "amount": 19.99,
    "currency": "USD",
    "status": { "code": "0", "message": "Success.", "trace_id": "b9f93f45" }
}"#;

#[tokio::test]
async fn payway_khqr_uses_the_qr_api_with_a_signed_request() {
    let (url, seen) = stub(vec![(
        "/api/payment-gateway/v1/payments/generate-qr",
        StatusCode::OK,
        serde_json::from_str(QR_RESPONSE).unwrap(),
    )])
    .await;

    let init = payway(&url)
        .create(&request("aba_khqr", 1999, Currency::Usd))
        .await
        .unwrap();

    assert_eq!(init.reference, "0123456789abcdef0123");
    assert_eq!(
        init.action,
        PaymentAction::Qr {
            qr_string: "00020101021230510016abaakhppxxx@abaa0115125021214532846".into(),
            deeplink: Some("abamobilebank://ababank.com?type=payway&qrcode=000201".into()),
        }
    );
    let sent = json_body(&seen, 0);
    assert_eq!(sent["req_time"], "20260312095439");
    assert_eq!(sent["merchant_id"], "ec000002");
    assert_eq!(sent["tran_id"], "0123456789abcdef0123");
    assert_eq!(sent["amount"], json!(19.99));
    assert_eq!(sent["currency"], "USD");
    assert_eq!(sent["payment_option"], "abapay_khqr");
    assert_eq!(sent["purchase_type"], "purchase");
    assert_eq!(sent["lifetime"], 10);
    assert_eq!(sent["qr_image_template"], "template3_color");
    assert_eq!(
        sent["callback_url"],
        "aHR0cHM6Ly9hcGkucmVhbml0LnRlc3QvYXBpL3YxL3dlYmhvb2tzL2FiYS1wYXl3YXk="
    );
    assert_eq!(
        sent["items"],
        "W3sibmFtZSI6IlJ1c3QgQmFzaWNzIiwicXVhbnRpdHkiOjEsInByaWNlIjoxOS45OX1d"
    );
    assert_eq!(sent["hash"], PAYWAY_QR_HASH);
}

#[tokio::test]
async fn payway_rejections_are_errors() {
    let (url, _) = stub(vec![(
        "/api/payment-gateway/v1/payments/generate-qr",
        StatusCode::OK,
        json!({ "status": { "code": "1", "message": "Wrong Hash", "trace_id": "x" } }),
    )])
    .await;

    let result = payway(&url)
        .create(&request("aba_khqr", 1999, Currency::Usd))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn payway_cards_post_a_signed_form_to_the_hosted_checkout() {
    let init = payway("https://checkout-sandbox.payway.com.kh")
        .create(&request("aba_cards", 2000, Currency::Usd))
        .await
        .unwrap();

    let PaymentAction::Redirect { url, fields } = init.action else {
        panic!("expected a redirect")
    };
    assert_eq!(
        url,
        "https://checkout-sandbox.payway.com.kh/api/payment-gateway/v1/payments/purchase"
    );
    let expected: BTreeMap<String, String> = [
        ("req_time", "20260312095439"),
        ("merchant_id", "ec000002"),
        ("tran_id", "0123456789abcdef0123"),
        ("amount", "20"),
        (
            "items",
            "W3sibmFtZSI6IlJ1c3QgQmFzaWNzIiwicXVhbnRpdHkiOjEsInByaWNlIjoyMH1d",
        ),
        ("email", "dara@example.com"),
        ("type", "purchase"),
        ("payment_option", "cards"),
        (
            "return_url",
            "aHR0cHM6Ly9hcGkucmVhbml0LnRlc3QvYXBpL3YxL3dlYmhvb2tzL2FiYS1wYXl3YXk=",
        ),
        (
            "cancel_url",
            "https://reanit.test/checkout/fedcba98-7654-3210-fedc-ba9876543210",
        ),
        (
            "continue_success_url",
            "https://reanit.test/checkout/fedcba98-7654-3210-fedc-ba9876543210",
        ),
        ("currency", "USD"),
        ("lifetime", "10"),
        ("hash", PAYWAY_CARDS_HASH),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    assert_eq!(fields, expected);
}

fn check_response(code: i64, status: &str) -> Value {
    json!({
        "data": {
            "payment_status_code": code,
            "total_amount": 19.99,
            "original_amount": 19.99,
            "refund_amount": 0,
            "discount_amount": 0,
            "payment_amount": 19.99,
            "payment_currency": "USD",
            "apv": "753786",
            "payment_status": status,
            "transaction_date": "2026-03-12 09:55:25"
        },
        "status": { "code": "00", "message": "Success!", "tran_id": "17394277693" }
    })
}

#[tokio::test]
async fn payway_check_transaction_maps_statuses() {
    for (code, status, expect_paid, expect_failed) in [
        (0, "APPROVED", true, false),
        (2, "PENDING", false, false),
        (3, "DECLINED", false, true),
        (7, "CANCELLED", false, true),
    ] {
        let (url, seen) = stub(vec![(
            "/api/payment-gateway/v1/payments/check-transaction-2",
            StatusCode::OK,
            check_response(code, status),
        )])
        .await;

        let result = payway(&url)
            .check(&payment(
                ProviderKind::AbaPayway,
                "0123456789abcdef0123",
                1999,
                Currency::Usd,
            ))
            .await
            .unwrap();

        match result {
            ProviderStatus::Paid {
                amount_minor,
                currency,
                provider_tran_id,
                ..
            } => {
                assert!(expect_paid, "{status}");
                assert_eq!(amount_minor, 1999);
                assert_eq!(currency, Currency::Usd);
                assert_eq!(provider_tran_id, "0123456789abcdef0123");
            }
            ProviderStatus::Failed { .. } => assert!(expect_failed, "{status}"),
            ProviderStatus::Pending => assert!(!expect_paid && !expect_failed, "{status}"),
        }
        let sent = json_body(&seen, 0);
        assert_eq!(
            sent,
            json!({
                "req_time": "20260312095439",
                "merchant_id": "ec000002",
                "tran_id": "0123456789abcdef0123",
                "hash": PAYWAY_CHECK_HASH,
            })
        );
    }
}

#[tokio::test]
async fn payway_unknown_transactions_are_pending() {
    let (url, _) = stub(vec![(
        "/api/payment-gateway/v1/payments/check-transaction-2",
        StatusCode::OK,
        json!({ "status": { "code": 6, "message": "Transaction not found", "tran_id": "x" } }),
    )])
    .await;

    let result = payway(&url)
        .check(&payment(ProviderKind::AbaPayway, "t", 100, Currency::Usd))
        .await
        .unwrap();

    assert!(matches!(result, ProviderStatus::Pending));
}

// Expected hashes computed independently with Python's hmac/hashlib from the
// field orders in PayWay's docs (QR API, Purchase, Check Transaction).
const PAYWAY_QR_HASH: &str =
    "OZdIBNGtZVhwvPR56lOIv4l/0D0x+zCzHdeVnk+muiRVKQmudqMYdzJ79SSQXRTBw40T/Xc6fDlnJURPhDUV8g==";
const PAYWAY_CARDS_HASH: &str =
    "U+OwfNdjqVFWtMTD8ius8zXjdCGzXxJCEzV8H9OVsNpQaJjJNQ+LNzoExc8e8YSyf4YLI/CbbfXAZYiMrjMaow==";
const PAYWAY_CHECK_HASH: &str =
    "gRZ7mqUaMu9qaA++2FGsxxvcU5aGkNp00MyKbEbNvQKt4d5j9zOrc+BI96HNxR9jPdt1/14EEbwOR2ILc3l5ng==";
