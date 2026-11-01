use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::{
    common::TestResponse,
    enrollment_api::{Shop, shop},
};

impl Shop {
    pub async fn add_to(&self, list: &str, course_id: &str) -> TestResponse {
        self.app
            .send(
                Method::POST,
                &format!("/me/{list}"),
                Some(json!({ "course_id": course_id })),
                &self.student,
            )
            .await
    }

    pub async fn list(&self, list: &str) -> Value {
        let res = self
            .app
            .get_authed(&format!("/me/{list}"), &self.student)
            .await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        res.body
    }
}

fn titles(list: &Value) -> Vec<String> {
    list["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["title"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn the_cart_lists_courses_with_a_total_per_currency() {
    let s = shop().await;
    let a = s.course("Rust", json!({ "price_minor": 1999 })).await;
    let b = s.course("Go", json!({ "price_minor": 1000 })).await;
    let c = s
        .course("Khmer", json!({ "price_minor": 40000, "currency": "KHR" }))
        .await;

    for id in [&a, &b, &c] {
        let res = s.add_to("cart", id).await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    }
    // Adding twice keeps one entry.
    s.add_to("cart", &a).await;

    let cart = s.list("cart").await;
    assert_eq!(titles(&cart), ["Khmer", "Go", "Rust"]);
    assert_eq!(
        cart["totals"],
        json!([
            { "currency": "KHR", "amount_minor": 40000 },
            { "currency": "USD", "amount_minor": 2999 },
        ])
    );
}

#[tokio::test]
async fn courses_can_be_removed_from_the_cart() {
    let s = shop().await;
    let a = s.course("Rust", json!({ "price_minor": 1999 })).await;
    s.add_to("cart", &a).await;

    let res = s
        .app
        .send(Method::DELETE, &format!("/me/cart/{a}"), None, &s.student)
        .await;

    assert_eq!(res.status, StatusCode::NO_CONTENT);
    assert_eq!(s.list("cart").await["data"], json!([]));
}

#[tokio::test]
async fn the_cart_only_takes_paid_courses_the_student_does_not_own() {
    let s = shop().await;
    let free = s.course("Free", json!({ "price_minor": 0 })).await;
    let paid = s.course("Paid", json!({ "price_minor": 500 })).await;

    assert_eq!(
        s.add_to("cart", &free).await.status,
        StatusCode::BAD_REQUEST
    );

    // The instructor can't buy their own course.
    let own = s
        .app
        .send(
            Method::POST,
            "/me/cart",
            Some(json!({ "course_id": paid })),
            &s.instructor,
        )
        .await;
    assert_eq!(own.status, StatusCode::BAD_REQUEST);

    s.enroll(&free).await;
    assert_eq!(
        s.add_to("cart", &free).await.status,
        StatusCode::CONFLICT,
        "already enrolled"
    );
    assert_eq!(
        s.add_to("cart", &uuid::Uuid::new_v4().to_string())
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_wishlist_saves_any_published_course() {
    let s = shop().await;
    let free = s.course("Free", json!({ "price_minor": 0 })).await;
    let paid = s.course("Paid", json!({ "price_minor": 500 })).await;

    assert_eq!(s.add_to("wishlist", &free).await.status, StatusCode::OK);
    assert_eq!(s.add_to("wishlist", &paid).await.status, StatusCode::OK);
    assert_eq!(titles(&s.list("wishlist").await), ["Paid", "Free"]);

    let res = s
        .app
        .send(
            Method::DELETE,
            &format!("/me/wishlist/{free}"),
            None,
            &s.student,
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    assert_eq!(titles(&s.list("wishlist").await), ["Paid"]);
}

#[tokio::test]
async fn the_course_page_shows_cart_and_wishlist_state() {
    let s = shop().await;
    let id = s.course("Paid", json!({ "price_minor": 500 })).await;
    let slug = s.list_slug().await;
    s.add_to("cart", &id).await;
    s.add_to("wishlist", &id).await;

    let page = s
        .app
        .get_authed(&format!("/courses/{slug}"), &s.student)
        .await
        .body;

    assert_eq!(
        page["viewer"],
        json!({ "enrolled": false, "in_cart": true, "in_wishlist": true })
    );
}

impl Shop {
    async fn list_slug(&self) -> String {
        self.app
            .request(Method::GET, "/courses", None, None, None)
            .await
            .body["data"][0]["slug"]
            .as_str()
            .unwrap()
            .to_string()
    }
}
