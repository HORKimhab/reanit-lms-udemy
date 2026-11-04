//! The background payment worker: polls providers and expires stale payments.

use axum::http::{Method, StatusCode};
use chrono::Duration;
use reanit_api::modules::{courses::model::Currency, payments::provider::ProviderKind};
use serde_json::{Value, json};

use crate::enrollment_api::{Shop, shop};

impl Shop {
    /// Order list as stored, without triggering a provider check.
    async fn stored_order(&self, order_id: &str) -> Value {
        let res = self.app.get_authed("/me/orders", &self.student).await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        res.body["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["id"] == order_id)
            .cloned()
            .expect("order is listed")
    }
}

#[tokio::test]
async fn the_worker_settles_a_bakong_payment_the_provider_confirms() {
    let s = shop().await;
    let (_, order) = s.pending_order(2000).await;
    let payment = s.pay(&order, "bakong_khqr").await.body;
    s.app.payments.settle(
        ProviderKind::Bakong,
        payment["reference"].as_str().unwrap(),
        2000,
        Currency::Usd,
    );

    let worked = s.app.payment_worker().run_once().await.unwrap();

    assert_eq!(worked, 1);
    assert_eq!(s.stored_order(&order).await["status"], "paid");
    assert_eq!(s.my_courses().await["data"][0]["course"]["title"], "Rust");
}

#[tokio::test]
async fn the_worker_expires_an_unpaid_payment_past_its_deadline() {
    let s = shop().await;
    let (_, order) = s.pending_order(2000).await;
    s.pay(&order, "bakong_khqr").await;
    let worker = s.app.payment_worker();

    s.app.clock.advance(Duration::minutes(5));
    assert_eq!(worker.run_once().await.unwrap(), 1, "still payable");
    assert_eq!(worker.run_once().await.unwrap(), 1, "still payable");
    s.app.clock.advance(Duration::minutes(6));
    assert_eq!(worker.run_once().await.unwrap(), 1, "checked and expired");

    // Nothing left to check, and the order can be paid afresh.
    assert_eq!(worker.run_once().await.unwrap(), 0);
    assert_eq!(s.stored_order(&order).await["status"], "pending");
    assert!(s.my_courses().await["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_worker_expires_orders_left_unpaid_for_a_day() {
    let mut s = shop().await;
    let (course, order) = s.pending_order(2000).await;

    s.app.clock.advance(Duration::hours(25));
    s.app.payment_worker().run_once().await.unwrap();
    s.student = s
        .app
        .login("student@example.com", "correct horse battery")
        .await
        .access_token();

    assert_eq!(s.stored_order(&order).await["status"], "expired");
    let res = s.pay(&order, "bakong_khqr").await;
    assert_eq!(res.status, StatusCode::CONFLICT, "{:?}", res.body);
    // The cart is untouched, so the student can simply check out again.
    assert_eq!(s.list("cart").await["data"][0]["id"], course);
    let again = s
        .app
        .send(
            Method::POST,
            "/orders",
            Some(json!({ "currency": "USD" })),
            &s.student,
        )
        .await;
    assert_eq!(again.status, StatusCode::CREATED, "{:?}", again.body);
}

#[tokio::test]
async fn an_old_order_with_a_payment_in_progress_is_not_expired() {
    let mut s = shop().await;
    let (_, order) = s.pending_order(2000).await;
    s.app
        .clock
        .advance(Duration::hours(24) - Duration::minutes(4));
    s.student = s
        .app
        .login("student@example.com", "correct horse battery")
        .await
        .access_token();
    s.pay(&order, "bakong_khqr").await;

    s.app.clock.advance(Duration::minutes(5));
    s.app.payment_worker().run_once().await.unwrap();

    assert_eq!(s.stored_order(&order).await["status"], "pending");
}
