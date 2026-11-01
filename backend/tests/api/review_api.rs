use axum::http::{Method, StatusCode};
use serde_json::json;

use crate::{
    common::TestApp,
    submit_api::{complete, setup, submit},
};

/// A course submitted for review; returns (app, instructor token, course base path, course id).
async fn pending_course() -> (TestApp, String, String, String) {
    let (app, token, base) = setup().await;
    complete(&app, &token, &base).await;
    assert_eq!(submit(&app, &token, &base).await.status, StatusCode::OK);
    let id = base.rsplit('/').next().unwrap().to_string();
    (app, token, base, id)
}

#[tokio::test]
async fn admins_see_courses_waiting_for_review() {
    let (app, _token, _base, id) = pending_course().await;
    let admin = app.admin("admin@example.com").await;

    let res = app
        .send(Method::GET, "/admin/courses?status=pending", None, &admin)
        .await;

    assert_eq!(res.status, StatusCode::OK);
    let ids: Vec<_> = res
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].clone())
        .collect();
    assert_eq!(ids, vec![json!(id)]);
    assert_eq!(res.body[0]["instructor_name"], "Dara Sok");
}

#[tokio::test]
async fn approving_publishes_the_course() {
    let (app, token, base, id) = pending_course().await;
    let admin = app.admin("admin@example.com").await;

    let res = app
        .send(
            Method::POST,
            &format!("/admin/courses/{id}/approve"),
            None,
            &admin,
        )
        .await;

    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["status"], "published");
    assert!(res.body["published_at"].is_string());
    let course = app.send(Method::GET, &base, None, &token).await;
    assert_eq!(course.body["status"], "published");
}

#[tokio::test]
async fn rejecting_returns_the_course_to_the_instructor_with_feedback() {
    let (app, token, base, id) = pending_course().await;
    let admin = app.admin("admin@example.com").await;

    let no_reason = app
        .send(
            Method::POST,
            &format!("/admin/courses/{id}/reject"),
            Some(json!({ "reason": " " })),
            &admin,
        )
        .await;
    assert_eq!(no_reason.status, StatusCode::BAD_REQUEST);

    let res = app
        .send(
            Method::POST,
            &format!("/admin/courses/{id}/reject"),
            Some(json!({ "reason": "Audio is too quiet" })),
            &admin,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["status"], "rejected");
    assert_eq!(res.body["rejection_reason"], "Audio is too quiet");

    // The instructor can edit and resubmit; resubmitting clears the feedback.
    let edit = app
        .send(
            Method::PATCH,
            &base,
            Some(json!({ "subtitle": "Louder now" })),
            &token,
        )
        .await;
    assert_eq!(edit.status, StatusCode::OK);
    let resubmitted = submit(&app, &token, &base).await;
    assert_eq!(resubmitted.body["status"], "pending");
    assert_eq!(
        resubmitted.body["rejection_reason"],
        serde_json::Value::Null
    );
}

#[tokio::test]
async fn only_pending_courses_can_be_reviewed_and_only_by_admins() {
    let (app, token, _base, id) = pending_course().await;
    let admin = app.admin("admin@example.com").await;

    let by_instructor = app
        .send(
            Method::POST,
            &format!("/admin/courses/{id}/approve"),
            None,
            &token,
        )
        .await;
    assert_eq!(by_instructor.status, StatusCode::FORBIDDEN);
    let list = app
        .send(Method::GET, "/admin/courses?status=pending", None, &token)
        .await;
    assert_eq!(list.status, StatusCode::FORBIDDEN);

    app.send(
        Method::POST,
        &format!("/admin/courses/{id}/approve"),
        None,
        &admin,
    )
    .await;
    let twice = app
        .send(
            Method::POST,
            &format!("/admin/courses/{id}/approve"),
            None,
            &admin,
        )
        .await;
    assert_eq!(twice.status, StatusCode::CONFLICT);
}
