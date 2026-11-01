mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

async fn setup() -> (TestApp, String, String) {
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
    let id = res.body["id"].as_str().unwrap().to_string();
    (app, token, id)
}

#[tokio::test]
async fn instructors_get_a_presigned_upload_url_for_a_course_thumbnail() {
    let (app, token, id) = setup().await;

    let res = app
        .send(
            Method::POST,
            &format!("/instructor/courses/{id}/thumbnail/upload-url"),
            Some(json!({ "content_type": "image/png" })),
            &token,
        )
        .await;

    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let key = res.body["key"].as_str().unwrap();
    assert!(key.starts_with(&format!("thumbnails/{id}/")), "{key}");
    assert!(key.ends_with(".png"));
    let url = res.body["upload_url"].as_str().unwrap();
    assert!(
        url.starts_with(&format!("http://localhost:9000/reanit-media/{key}?")),
        "{url}"
    );
    assert!(url.contains("X-Amz-Signature="));
    assert_eq!(res.body["expires_in_secs"], 900);
}

#[tokio::test]
async fn only_images_can_be_uploaded_as_thumbnails() {
    let (app, token, id) = setup().await;

    let res = app
        .send(
            Method::POST,
            &format!("/instructor/courses/{id}/thumbnail/upload-url"),
            Some(json!({ "content_type": "application/x-msdownload" })),
            &token,
        )
        .await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_uploaded_thumbnail_is_attached_and_exposed_as_a_public_url() {
    let (app, token, id) = setup().await;
    let upload = app
        .send(
            Method::POST,
            &format!("/instructor/courses/{id}/thumbnail/upload-url"),
            Some(json!({ "content_type": "image/jpeg" })),
            &token,
        )
        .await;
    let key = upload.body["key"].as_str().unwrap();

    let res = app
        .send(
            Method::PUT,
            &format!("/instructor/courses/{id}/thumbnail"),
            Some(json!({ "key": key })),
            &token,
        )
        .await;

    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(
        res.body["thumbnail_url"],
        format!("http://cdn.test/reanit-media/{key}")
    );
    let course = app
        .send(
            Method::GET,
            &format!("/instructor/courses/{id}"),
            None,
            &token,
        )
        .await;
    assert_eq!(course.body["thumbnail_url"], res.body["thumbnail_url"]);
}

#[tokio::test]
async fn a_thumbnail_key_from_another_course_is_rejected() {
    let (app, token, id) = setup().await;

    for key in [
        "thumbnails/someone-else/x.png",
        "../secrets",
        "videos/x.mp4",
    ] {
        let res = app
            .send(
                Method::PUT,
                &format!("/instructor/courses/{id}/thumbnail"),
                Some(json!({ "key": key })),
                &token,
            )
            .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{key}");
    }
}

#[tokio::test]
async fn courses_without_a_thumbnail_have_a_null_url() {
    let (app, token, id) = setup().await;
    let course = app
        .send(
            Method::GET,
            &format!("/instructor/courses/{id}"),
            None,
            &token,
        )
        .await;
    assert_eq!(course.body["thumbnail_url"], serde_json::Value::Null);
}
