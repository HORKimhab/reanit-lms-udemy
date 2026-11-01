
use axum::http::{Method, StatusCode};
use crate::common::TestApp;
use serde_json::json;

async fn verified_student(app: &TestApp, email: &str) -> String {
    let token = app
        .register("Dara Sok", email, "correct horse battery")
        .await
        .access_token();
    let verification = app.last_verification_token(email);
    app.post("/auth/verify-email", json!({ "token": verification }))
        .await;
    token
}

#[tokio::test]
async fn categories_are_listed_publicly() {
    let app = TestApp::new();

    let res = app
        .request(Method::GET, "/categories", None, None, None)
        .await;

    assert_eq!(res.status, StatusCode::OK);
    let categories = res.body.as_array().unwrap();
    assert!(!categories.is_empty());
    assert!(categories[0]["slug"].is_string());
    assert!(categories[0]["name"].is_string());
}

#[tokio::test]
async fn becoming_an_instructor_creates_a_public_profile() {
    let app = TestApp::new();
    let token = verified_student(&app, "dara@example.com").await;

    let res = app
        .request(
            Method::POST,
            "/me/become-instructor",
            Some(json!({ "headline": "Senior Rust engineer", "bio": "I teach backend development." })),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["role"], "instructor");

    let profile = app.get_authed("/me/instructor-profile", &token).await;
    assert_eq!(profile.status, StatusCode::OK);
    assert_eq!(profile.body["headline"], "Senior Rust engineer");
    assert_eq!(profile.body["bio"], "I teach backend development.");
    assert_eq!(profile.body["website"], serde_json::Value::Null);
}

#[tokio::test]
async fn becoming_an_instructor_requires_a_headline() {
    let app = TestApp::new();
    let token = verified_student(&app, "dara@example.com").await;

    let res = app
        .request(
            Method::POST,
            "/me/become-instructor",
            Some(json!({ "headline": "  " })),
            Some(&token),
            None,
        )
        .await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn instructors_can_update_their_profile_but_students_have_none() {
    let app = TestApp::new();
    let student = verified_student(&app, "student@example.com").await;
    assert_eq!(
        app.get_authed("/me/instructor-profile", &student)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    let token = verified_student(&app, "dara@example.com").await;
    app.request(
        Method::POST,
        "/me/become-instructor",
        Some(json!({ "headline": "Rust dev" })),
        Some(&token),
        None,
    )
    .await;

    let res = app
        .request(
            Method::PUT,
            "/me/instructor-profile",
            Some(
                json!({ "headline": "Rust & Go dev", "bio": "Hi", "website": "https://dara.dev" }),
            ),
            Some(&token),
            None,
        )
        .await;

    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["headline"], "Rust & Go dev");
    assert_eq!(res.body["website"], "https://dara.dev");
}
