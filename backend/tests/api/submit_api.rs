use crate::common::TestApp;
use axum::http::{Method, StatusCode};
use serde_json::json;

pub async fn setup() -> (TestApp, String, String) {
    let app = TestApp::new();
    let token = app.instructor("dara@example.com").await;
    let res = app
        .send(
            Method::POST,
            "/instructor/courses",
            Some(json!({ "title": "Rust" })),
            &token,
        )
        .await;
    let base = format!("/instructor/courses/{}", res.body["id"].as_str().unwrap());
    (app, token, base)
}

/// Fills in everything a course needs to be submitted.
pub async fn complete(app: &TestApp, token: &str, base: &str) {
    let patch = app
        .send(
            Method::PATCH,
            base,
            Some(json!({
                "subtitle": "Learn Rust",
                "description": "A practical introduction to Rust.",
                "category_id": app.category_id(),
                "outcomes": ["Write Rust"],
            })),
            token,
        )
        .await;
    assert_eq!(patch.status, StatusCode::OK);
    let upload = app
        .send(
            Method::POST,
            &format!("{base}/thumbnail/upload-url"),
            Some(json!({ "content_type": "image/png" })),
            token,
        )
        .await;
    app.send(
        Method::PUT,
        &format!("{base}/thumbnail"),
        Some(json!({ "key": upload.body["key"] })),
        token,
    )
    .await;
    let section = app
        .send(
            Method::POST,
            &format!("{base}/sections"),
            Some(json!({ "title": "Intro" })),
            token,
        )
        .await;
    let section = section.body["id"].as_str().unwrap().to_string();
    let lecture = app
        .send(
            Method::POST,
            &format!("{base}/sections/{section}/lectures"),
            Some(json!({ "title": "Read me", "kind": "article" })),
            token,
        )
        .await;
    let lecture = lecture.body["id"].as_str().unwrap().to_string();
    app.send(
        Method::PATCH,
        &format!("{base}/lectures/{lecture}"),
        Some(json!({ "article_body": "Hello Rust" })),
        token,
    )
    .await;
}

pub async fn submit(app: &TestApp, token: &str, base: &str) -> crate::common::TestResponse {
    app.send(Method::POST, &format!("{base}/submit"), None, token)
        .await
}

#[tokio::test]
async fn an_incomplete_course_cannot_be_submitted_and_the_reasons_are_listed() {
    let (app, token, base) = setup().await;

    let res = submit(&app, &token, &base).await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    let message = res.body["error"]["message"].as_str().unwrap();
    for missing in [
        "subtitle",
        "description",
        "category",
        "thumbnail",
        "at least one lecture",
    ] {
        assert!(message.contains(missing), "{missing:?} not in {message:?}");
    }
}

#[tokio::test]
async fn empty_sections_and_lectures_without_content_block_submission() {
    let (app, token, base) = setup().await;
    complete(&app, &token, &base).await;
    let empty = app
        .send(
            Method::POST,
            &format!("{base}/sections"),
            Some(json!({ "title": "Empty" })),
            &token,
        )
        .await;
    let empty = empty.body["id"].as_str().unwrap().to_string();

    let res = submit(&app, &token, &base).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert!(
        res.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("\"Empty\" has no lectures")
    );

    app.send(
        Method::POST,
        &format!("{base}/sections/{empty}/lectures"),
        Some(json!({ "title": "Watch", "kind": "video" })),
        &token,
    )
    .await;
    let res = submit(&app, &token, &base).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert!(
        res.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("\"Watch\" has no video")
    );
}

#[tokio::test]
async fn a_complete_course_is_submitted_for_review_and_locked() {
    let (app, token, base) = setup().await;
    complete(&app, &token, &base).await;

    let res = submit(&app, &token, &base).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["status"], "pending");

    let edit = app
        .send(
            Method::PATCH,
            &base,
            Some(json!({ "title": "Changed" })),
            &token,
        )
        .await;
    assert_eq!(edit.status, StatusCode::CONFLICT);
    let add = app
        .send(
            Method::POST,
            &format!("{base}/sections"),
            Some(json!({ "title": "More" })),
            &token,
        )
        .await;
    assert_eq!(add.status, StatusCode::CONFLICT);
    assert_eq!(
        submit(&app, &token, &base).await.status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.send(Method::DELETE, &base, None, &token).await.status,
        StatusCode::CONFLICT
    );
}
