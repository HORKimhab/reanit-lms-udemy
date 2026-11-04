//! Checkout and payments through the HTTP API, with a scripted payment provider.

use axum::http::{Method, StatusCode};
use chrono::Duration;
use reanit_api::modules::{
    courses::model::Currency,
    payments::provider::{PaymentAction, ProviderKind},
};
use serde_json::{Value, json};

use crate::{
    common::TestResponse,
    enrollment_api::{Shop, shop},
};

impl Shop {
    async fn checkout(&self, body: Value) -> TestResponse {
        self.app
            .send(Method::POST, "/orders", Some(body), &self.student)
            .await
    }

    pub(crate) async fn pay(&self, order_id: &str, method: &str) -> TestResponse {
        self.app
            .send(
                Method::POST,
                &format!("/orders/{order_id}/payments"),
                Some(json!({ "method": method })),
                &self.student,
            )
            .await
    }

    async fn order(&self, order_id: &str) -> Value {
        let res = self
            .app
            .get_authed(&format!("/orders/{order_id}"), &self.student)
            .await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        res.body
    }

    async fn methods(&self, currency: &str) -> Vec<String> {
        let res = self
            .app
            .get_authed(
                &format!("/payments/methods?currency={currency}"),
                &self.student,
            )
            .await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        res.body["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["code"].as_str().unwrap().to_string())
            .collect()
    }

    /// A paid course in the student's cart plus a pending order for it.
    pub(crate) async fn pending_order(&self, price_minor: i64) -> (String, String) {
        let course = self
            .course("Rust", json!({ "price_minor": price_minor }))
            .await;
        self.add_to("cart", &course).await;
        let order = self.checkout(json!({ "currency": "USD" })).await;
        assert_eq!(order.status, StatusCode::CREATED, "{:?}", order.body);
        (course, order.body["id"].as_str().unwrap().to_string())
    }
}

#[tokio::test]
async fn payment_methods_are_listed_per_currency_in_admin_order() {
    let s = shop().await;

    assert_eq!(
        s.methods("USD").await,
        ["bakong_khqr", "aba_khqr", "aba_cards"]
    );
    assert_eq!(s.methods("KHR").await, ["bakong_khqr", "aba_khqr"]);

    let res = s
        .app
        .send(
            Method::PATCH,
            "/admin/payment-methods/aba_khqr",
            Some(json!({ "enabled": false })),
            &s.admin,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let res = s
        .app
        .send(
            Method::PATCH,
            "/admin/payment-methods/aba_cards",
            Some(json!({ "sort_order": 0, "label": "Visa / Mastercard" })),
            &s.admin,
        )
        .await;
    assert_eq!(res.body["label"], "Visa / Mastercard");

    assert_eq!(s.methods("USD").await, ["aba_cards", "bakong_khqr"]);
}

#[tokio::test]
async fn only_admins_manage_payment_methods() {
    let s = shop().await;
    let res = s
        .app
        .send(
            Method::PATCH,
            "/admin/payment-methods/aba_khqr",
            Some(json!({ "enabled": false })),
            &s.student,
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    let list = s.app.get_authed("/admin/payment-methods", &s.admin).await;
    assert_eq!(list.body["data"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn methods_whose_provider_is_not_configured_are_hidden() {
    let s = shop().await;
    s.app.payments.unconfigure(ProviderKind::AbaPayway);

    assert_eq!(s.methods("USD").await, ["bakong_khqr"]);
}

#[tokio::test]
async fn checkout_prices_cart_courses_of_one_currency_from_the_database() {
    let s = shop().await;
    let rust = s.course("Rust", json!({ "price_minor": 2000 })).await;
    let go = s.course("Go", json!({ "price_minor": 1000 })).await;
    let khmer = s
        .course("Khmer", json!({ "price_minor": 40000, "currency": "KHR" }))
        .await;
    s.create_coupon(&rust, json!({ "code": "HALF", "discount_percent": 50 }))
        .await;
    for id in [&rust, &go, &khmer] {
        s.add_to("cart", id).await;
    }

    let res = s
        .checkout(json!({ "currency": "USD", "coupon_codes": ["half"] }))
        .await;

    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    let order = &res.body;
    assert_eq!(order["status"], "pending");
    assert_eq!(order["currency"], "USD");
    assert_eq!(order["subtotal_minor"], 3000);
    assert_eq!(order["total_minor"], 2000);
    let items = order["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let rust_item = items
        .iter()
        .find(|i| i["course_id"] == rust.as_str())
        .unwrap();
    assert_eq!(rust_item["title"], "Rust");
    assert_eq!(rust_item["price_minor"], 1000);
    assert_eq!(rust_item["coupon_code"], "HALF");
    assert!(order["payment"].is_null());
}

#[tokio::test]
async fn buy_now_checks_out_specific_courses() {
    let s = shop().await;
    let rust = s.course("Rust", json!({ "price_minor": 2000 })).await;

    let res = s.checkout(json!({ "course_ids": [rust] })).await;

    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    assert_eq!(res.body["total_minor"], 2000);
}

#[tokio::test]
async fn checkout_rejects_empty_mixed_or_owned_selections() {
    let s = shop().await;
    assert_eq!(
        s.checkout(json!({ "currency": "USD" })).await.status,
        StatusCode::BAD_REQUEST,
        "empty cart"
    );

    let usd = s.course("Rust", json!({ "price_minor": 2000 })).await;
    let khr = s
        .course("Khmer", json!({ "price_minor": 40000, "currency": "KHR" }))
        .await;
    assert_eq!(
        s.checkout(json!({ "course_ids": [usd, khr] })).await.status,
        StatusCode::BAD_REQUEST,
        "one currency per order"
    );

    let free = s.course("Free", json!({ "price_minor": 0 })).await;
    s.enroll(&free).await;
    assert_eq!(
        s.checkout(json!({ "course_ids": [free] })).await.status,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn a_fully_discounted_order_is_completed_without_payment() {
    let s = shop().await;
    let rust = s.course("Rust", json!({ "price_minor": 2000 })).await;
    s.create_coupon(&rust, json!({ "code": "FREE", "discount_percent": 100 }))
        .await;
    s.add_to("cart", &rust).await;

    let res = s
        .checkout(json!({ "currency": "USD", "coupon_codes": ["FREE"] }))
        .await;

    assert_eq!(res.body["status"], "paid");
    assert_eq!(s.my_courses().await["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn paying_with_bakong_returns_a_khqr_for_the_order_amount() {
    let s = shop().await;
    let (_, order) = s.pending_order(1999).await;

    let res = s.pay(&order, "bakong_khqr").await;

    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    let p = &res.body;
    assert_eq!(p["status"], "pending");
    assert_eq!(p["method"], "bakong_khqr");
    assert_eq!(p["amount_minor"], 1999);
    assert_eq!(p["currency"], "USD");
    assert!(p["qr"]["string"].as_str().unwrap().starts_with("000201"));
    assert!(p["qr"]["svg"].as_str().unwrap().contains("</svg>"));
    assert!(p["expires_at"].is_string());

    let sent = s.app.payments.requests(ProviderKind::Bakong);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].amount_minor, 1999, "amount comes from the order");
    assert_eq!(sent[0].currency, Currency::Usd);

    // Asking again while it's still valid returns the same payment.
    let again = s.pay(&order, "bakong_khqr").await;
    assert_eq!(again.status, StatusCode::CREATED);
    assert_eq!(again.body["id"], p["id"]);
    assert_eq!(s.app.payments.requests(ProviderKind::Bakong).len(), 1);
}

#[tokio::test]
async fn card_payments_redirect_to_the_hosted_checkout() {
    let s = shop().await;
    let (_, order) = s.pending_order(1999).await;

    let res = s.pay(&order, "aba_cards").await;

    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    assert_eq!(res.body["redirect"]["url"], "https://pay.test/checkout");
    assert_eq!(
        res.body["redirect"]["fields"]["tran_id"],
        res.body["reference"]
    );
}

#[tokio::test]
async fn a_confirmed_payment_enrolls_the_student_once() {
    let s = shop().await;
    let rust = s.course("Rust", json!({ "price_minor": 2000 })).await;
    let coupon = s
        .create_coupon(&rust, json!({ "code": "TEN", "discount_percent": 10 }))
        .await;
    s.add_to("cart", &rust).await;
    let order = s
        .checkout(json!({ "currency": "USD", "coupon_codes": ["TEN"] }))
        .await
        .body["id"]
        .as_str()
        .unwrap()
        .to_string();
    let payment = s.pay(&order, "bakong_khqr").await.body;

    // Not paid yet.
    assert_eq!(s.order(&order).await["status"], "pending");

    s.app.payments.settle(
        ProviderKind::Bakong,
        payment["reference"].as_str().unwrap(),
        1800,
        Currency::Usd,
    );
    s.app.clock.advance(Duration::seconds(5));
    let paid = s.order(&order).await;

    assert_eq!(paid["status"], "paid", "{paid:?}");
    assert_eq!(paid["payment"]["status"], "paid");
    assert!(paid["paid_at"].is_string());
    let mine = s.my_courses().await;
    assert_eq!(mine["data"][0]["course"]["title"], "Rust");
    assert_eq!(
        s.list("cart").await["data"],
        json!([]),
        "bought courses leave the cart"
    );
    let coupons = s
        .app
        .get_authed(
            &format!("/instructor/courses/{rust}/coupons"),
            &s.instructor,
        )
        .await;
    assert_eq!(coupons.body["data"][0]["used_count"], 1);
    assert_eq!(coupon.status, StatusCode::CREATED);
    let receipt = s
        .app
        .mailer
        .sent
        .lock()
        .unwrap()
        .iter()
        .any(|m| m.to == "student@example.com" && m.subject.contains("receipt"));
    assert!(receipt, "a receipt email is sent");

    // Polling again changes nothing.
    s.app.clock.advance(Duration::seconds(5));
    assert_eq!(s.order(&order).await["status"], "paid");
    assert_eq!(s.my_courses().await["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_payment_for_the_wrong_amount_does_not_enroll() {
    let s = shop().await;
    let (_, order) = s.pending_order(2000).await;
    let payment = s.pay(&order, "bakong_khqr").await.body;

    s.app.payments.settle(
        ProviderKind::Bakong,
        payment["reference"].as_str().unwrap(),
        1,
        Currency::Usd,
    );
    s.app.clock.advance(Duration::seconds(5));
    let after = s.order(&order).await;

    assert_eq!(after["status"], "pending");
    assert_eq!(after["payment"]["status"], "failed");
    assert_eq!(s.my_courses().await["data"], json!([]));
}

#[tokio::test]
async fn an_unpaid_payment_expires_and_a_new_one_can_be_started() {
    let s = shop().await;
    let (_, order) = s.pending_order(2000).await;
    let first = s.pay(&order, "bakong_khqr").await.body;

    s.app.clock.advance(Duration::minutes(11));
    let after = s.order(&order).await;
    assert_eq!(after["payment"]["status"], "expired");

    let second = s.pay(&order, "bakong_khqr").await.body;
    assert_ne!(second["id"], first["id"]);
    assert_eq!(second["status"], "pending");
}

#[tokio::test]
async fn the_provider_is_not_asked_more_than_every_few_seconds() {
    let s = shop().await;
    let (_, order) = s.pending_order(2000).await;
    s.pay(&order, "bakong_khqr").await;

    s.order(&order).await;
    s.order(&order).await;
    assert_eq!(s.app.payments.checks(ProviderKind::Bakong), 1);
    s.app.clock.advance(Duration::seconds(4));
    s.order(&order).await;
    assert_eq!(s.app.payments.checks(ProviderKind::Bakong), 2);
}

#[tokio::test]
async fn orders_are_private_and_methods_must_fit_the_currency() {
    let s = shop().await;
    let khmer = s
        .course("Khmer", json!({ "price_minor": 40000, "currency": "KHR" }))
        .await;
    let order = s.checkout(json!({ "course_ids": [khmer] })).await.body["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(
        s.pay(&order, "aba_cards").await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(s.pay(&order, "nope").await.status, StatusCode::BAD_REQUEST);

    let stranger = s.app.verified_student("stranger@example.com").await;
    let res = s
        .app
        .get_authed(&format!("/orders/{order}"), &stranger)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn my_orders_lists_newest_first() {
    let s = shop().await;
    let (_, order) = s.pending_order(2000).await;

    let res = s.app.get_authed("/me/orders", &s.student).await;

    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["data"][0]["id"], order.as_str());
}

#[tokio::test]
async fn the_aba_callback_triggers_verification_with_the_provider() {
    let s = shop().await;
    let (_, order) = s.pending_order(2000).await;
    let payment = s.pay(&order, "aba_khqr").await.body;
    let tran_id = payment["reference"].as_str().unwrap();
    assert!(matches!(
        s.app.payments.last_action(ProviderKind::AbaPayway),
        Some(PaymentAction::Qr { .. })
    ));

    // The callback alone proves nothing...
    let res = s
        .app
        .post(
            "/webhooks/aba-payway",
            json!({ "tran_id": tran_id, "status": 0 }),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(s.my_courses().await["data"], json!([]));

    // ...it only makes us ask PayWay, which is the source of truth.
    s.app
        .payments
        .settle(ProviderKind::AbaPayway, tran_id, 2000, Currency::Usd);
    s.app
        .post(
            "/webhooks/aba-payway",
            json!({ "tran_id": tran_id, "status": 0 }),
        )
        .await;
    assert_eq!(s.my_courses().await["data"].as_array().unwrap().len(), 1);

    let unknown = s
        .app
        .post(
            "/webhooks/aba-payway",
            json!({ "tran_id": "nope", "status": 0 }),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::OK);
}
