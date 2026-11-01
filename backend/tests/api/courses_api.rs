
use axum::http::{Method, StatusCode};
use crate::common::{TestApp, TestResponse};
use serde_json::{Value, json};

async fn create_course(app: &TestApp, token: &str, body: Value) -> TestResponse {
    app.send(Method::POST, "/instructor/courses", Some(body), token)
        .await
}

async fn new_course(app: &TestApp, token: &str, title: &str) -> String {
    let res = create_course(app, token, json!({ "title": title })).await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    res.body["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn an_instructor_creates_a_draft_course_with_sensible_defaults() {
    let app = TestApp::new();
    let token = app.instructor("dara@example.com").await;

    let res = create_course(
        &app,
        &token,
        json!({ "title": "  Rust for Web Developers!  ", "category_id": app.category_id() }),
    )
    .await;

    assert_eq!(res.status, StatusCode::CREATED);
    let c = &res.body;
    assert_eq!(c["title"], "Rust for Web Developers!");
    assert!(
        c["slug"]
            .as_str()
            .unwrap()
            .starts_with("rust-for-web-developers-")
    );
    assert_eq!(c["status"], "draft");
    assert_eq!(c["category_id"], app.category_id());
    assert_eq!(c["price_minor"], 0);
    assert_eq!(c["currency"], "USD");
    assert_eq!(c["level"], "all");
    assert_eq!(c["language"], "en");
    assert_eq!(c["outcomes"], json!([]));
}

#[tokio::test]
async fn students_cannot_create_courses() {
    let app = TestApp::new();
    let token = app.verified_student("student@example.com").await;

    let res = create_course(&app, &token, json!({ "title": "Sneaky" })).await;

    assert_eq!(res.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn creating_a_course_validates_title_and_category() {
    let app = TestApp::new();
    let token = app.instructor("dara@example.com").await;

    let blank = create_course(&app, &token, json!({ "title": "   " })).await;
    assert_eq!(blank.status, StatusCode::BAD_REQUEST);

    let long = create_course(&app, &token, json!({ "title": "x".repeat(61) })).await;
    assert_eq!(long.status, StatusCode::BAD_REQUEST);

    let unknown = create_course(
        &app,
        &token,
        json!({ "title": "Ok", "category_id": uuid::Uuid::new_v4().to_string() }),
    )
    .await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn instructors_only_see_and_edit_their_own_courses() {
    let app = TestApp::new();
    let dara = app.instructor("dara@example.com").await;
    let sok = app.instructor("sok@example.com").await;
    let course = new_course(&app, &dara, "Dara's course").await;
    new_course(&app, &sok, "Sok's course").await;

    let mine = app
        .send(Method::GET, "/instructor/courses", None, &dara)
        .await;
    assert_eq!(mine.status, StatusCode::OK);
    let titles: Vec<_> = mine
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["title"].clone())
        .collect();
    assert_eq!(titles, vec![json!("Dara's course")]);

    let path = format!("/instructor/courses/{course}");
    assert_eq!(
        app.send(Method::GET, &path, None, &sok).await.status,
        StatusCode::NOT_FOUND
    );
    let edit = app
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "title": "Mine now" })),
            &sok,
        )
        .await;
    assert_eq!(edit.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn updating_a_course_changes_only_the_given_fields() {
    let app = TestApp::new();
    let token = app.instructor("dara@example.com").await;
    let course = new_course(&app, &token, "Rust Basics").await;
    let path = format!("/instructor/courses/{course}");
    let slug_before = app.send(Method::GET, &path, None, &token).await.body["slug"].clone();

    let res = app
        .send(
            Method::PATCH,
            &path,
            Some(json!({
                "title": "Rust Basics 2026",
                "subtitle": "From zero to production",
                "description": "A practical course.",
                "category_id": app.category_id(),
                "level": "beginner",
                "language": "km",
                "price_minor": 1999,
                "currency": "USD",
                "outcomes": ["Write safe Rust", "Build an API"],
                "requirements": ["Basic programming"],
            })),
            &token,
        )
        .await;

    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["title"], "Rust Basics 2026");
    assert_eq!(res.body["subtitle"], "From zero to production");
    assert_eq!(res.body["level"], "beginner");
    assert_eq!(res.body["language"], "km");
    assert_eq!(res.body["price_minor"], 1999);
    assert_eq!(
        res.body["outcomes"],
        json!(["Write safe Rust", "Build an API"])
    );
    assert_eq!(
        res.body["slug"], slug_before,
        "slug is stable across title changes"
    );

    let partial = app
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "currency": "KHR" })),
            &token,
        )
        .await;
    assert_eq!(partial.body["currency"], "KHR");
    assert_eq!(partial.body["title"], "Rust Basics 2026");
}

#[tokio::test]
async fn invalid_course_updates_are_rejected() {
    let app = TestApp::new();
    let token = app.instructor("dara@example.com").await;
    let course = new_course(&app, &token, "Rust Basics").await;
    let path = format!("/instructor/courses/{course}");

    for body in [
        json!({ "price_minor": -1 }),
        json!({ "currency": "EUR" }),
        json!({ "level": "wizard" }),
        json!({ "subtitle": "x".repeat(121) }),
        json!({ "outcomes": ["x".repeat(161)] }),
        json!({ "category_id": uuid::Uuid::new_v4().to_string() }),
    ] {
        let res = app
            .send(Method::PATCH, &path, Some(body.clone()), &token)
            .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
async fn admins_can_view_and_edit_any_course() {
    let app = TestApp::new();
    let dara = app.instructor("dara@example.com").await;
    let admin = app.admin("admin@example.com").await;
    let course = new_course(&app, &dara, "Rust Basics").await;
    let path = format!("/instructor/courses/{course}");

    let res = app
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "subtitle": "Fixed typo" })),
            &admin,
        )
        .await;

    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["subtitle"], "Fixed typo");
}

#[tokio::test]
async fn draft_courses_can_be_deleted() {
    let app = TestApp::new();
    let token = app.instructor("dara@example.com").await;
    let course = new_course(&app, &token, "Throwaway").await;
    let path = format!("/instructor/courses/{course}");

    assert_eq!(
        app.send(Method::DELETE, &path, None, &token).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.send(Method::GET, &path, None, &token).await.status,
        StatusCode::NOT_FOUND
    );
}
