use axum::http::{Method, StatusCode};
use chrono::Duration;
use reanit_api::infra::clock::Clock;
use serde_json::{Value, json};

use crate::{
    common::TestResponse,
    enrollment_api::{Shop, shop},
};

impl Shop {
    pub async fn create_coupon(&self, course_id: &str, body: Value) -> TestResponse {
        self.app
            .send(
                Method::POST,
                &format!("/instructor/courses/{course_id}/coupons"),
                Some(body),
                &self.instructor,
            )
            .await
    }

    async fn cart_with(&self, coupons: &str) -> Value {
        let res = self
            .app
            .get_authed(&format!("/me/cart?coupons={coupons}"), &self.student)
            .await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        res.body
    }
}

#[tokio::test]
async fn instructors_create_and_list_coupons_for_their_course() {
    let s = shop().await;
    let id = s.course("Rust", json!({ "price_minor": 2000 })).await;

    let res = s
        .create_coupon(&id, json!({ "code": "launch-50", "discount_percent": 50 }))
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    assert_eq!(res.body["code"], "LAUNCH-50", "codes are case-insensitive");
    assert_eq!(res.body["used_count"], 0);

    let dup = s
        .create_coupon(&id, json!({ "code": "LAUNCH-50", "discount_percent": 10 }))
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);

    let list = s
        .app
        .get_authed(&format!("/instructor/courses/{id}/coupons"), &s.instructor)
        .await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn coupon_input_is_validated() {
    let s = shop().await;
    let id = s.course("Rust", json!({ "price_minor": 2000 })).await;

    for bad in [
        json!({ "code": "X", "discount_percent": 10 }),
        json!({ "code": "has space", "discount_percent": 10 }),
        json!({ "code": "OK123", "discount_percent": 0 }),
        json!({ "code": "OK123", "discount_percent": 101 }),
        json!({ "code": "OK123", "discount_percent": 10, "max_uses": 0 }),
    ] {
        let res = s.create_coupon(&id, bad.clone()).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{bad}");
    }
}

#[tokio::test]
async fn only_the_course_instructor_manages_its_coupons() {
    let s = shop().await;
    let id = s.course("Rust", json!({ "price_minor": 2000 })).await;
    let other = s.app.instructor("other@example.com").await;

    let res = s
        .app
        .send(
            Method::POST,
            &format!("/instructor/courses/{id}/coupons"),
            Some(json!({ "code": "STEAL", "discount_percent": 100 })),
            &other,
        )
        .await;

    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_valid_coupon_discounts_only_its_course_in_the_cart() {
    let s = shop().await;
    let rust = s.course("Rust", json!({ "price_minor": 2000 })).await;
    let go = s.course("Go", json!({ "price_minor": 1000 })).await;
    s.create_coupon(&rust, json!({ "code": "HALF", "discount_percent": 50 }))
        .await;
    s.add_to("cart", &rust).await;
    s.add_to("cart", &go).await;

    let cart = s.cart_with("half").await;

    assert_eq!(
        cart["totals"],
        json!([{ "currency": "USD", "amount_minor": 2000 }])
    );
    let rust_line = cart["pricing"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["course_id"] == rust.as_str())
        .unwrap()
        .clone();
    assert_eq!(
        rust_line,
        json!({ "course_id": rust, "list_price_minor": 2000, "price_minor": 1000, "coupon_code": "HALF" })
    );
}

#[tokio::test]
async fn expired_used_up_or_unknown_coupons_are_ignored() {
    let s = shop().await;
    let id = s.course("Rust", json!({ "price_minor": 2000 })).await;
    let expires = (s.app.clock.now() + Duration::minutes(10)).to_rfc3339();
    s.create_coupon(
        &id,
        json!({ "code": "SOON", "discount_percent": 50, "expires_at": expires }),
    )
    .await;
    s.add_to("cart", &id).await;

    assert_eq!(s.cart_with("SOON").await["totals"][0]["amount_minor"], 1000);
    assert_eq!(s.cart_with("NOPE").await["totals"][0]["amount_minor"], 2000);
    s.app.clock.advance(Duration::minutes(11));
    assert_eq!(s.cart_with("SOON").await["totals"][0]["amount_minor"], 2000);
    assert_eq!(
        s.cart_with("SOON").await["invalid_coupons"],
        json!(["SOON"])
    );
}

#[tokio::test]
async fn coupons_can_be_deleted() {
    let s = shop().await;
    let id = s.course("Rust", json!({ "price_minor": 2000 })).await;
    let coupon = s
        .create_coupon(&id, json!({ "code": "GONE", "discount_percent": 20 }))
        .await;

    let res = s
        .app
        .send(
            Method::DELETE,
            &format!(
                "/instructor/courses/{id}/coupons/{}",
                coupon.body["id"].as_str().unwrap()
            ),
            None,
            &s.instructor,
        )
        .await;

    assert_eq!(res.status, StatusCode::NO_CONTENT);
    s.add_to("cart", &id).await;
    assert_eq!(s.cart_with("GONE").await["totals"][0]["amount_minor"], 2000);
}
