mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use reanit_api::modules::users::{model::Role, repo::UserRepo};
use serde_json::json;
use uuid::Uuid;

async fn become_instructor(app: &TestApp, token: &str) -> common::TestResponse {
    app.request(
        Method::POST,
        "/me/become-instructor",
        None,
        Some(token),
        None,
    )
    .await
}

async fn set_role(app: &TestApp, token: &str, user_id: &str, role: &str) -> common::TestResponse {
    app.request(
        Method::PATCH,
        &format!("/admin/users/{user_id}/role"),
        Some(json!({ "role": role })),
        Some(token),
        None,
    )
    .await
}

#[tokio::test]
async fn a_student_must_verify_their_email_before_becoming_an_instructor() {
    let app = TestApp::new();
    let token = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .access_token();

    let refused = become_instructor(&app, &token).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);

    let verification = app.last_verification_token("dara@example.com");
    app.post("/auth/verify-email", json!({ "token": verification }))
        .await;

    let res = become_instructor(&app, &token).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["role"], "instructor");
    // Role changes apply immediately, without a new access token.
    assert_eq!(
        app.get_authed("/auth/me", &token).await.body["role"],
        "instructor"
    );
}

#[tokio::test]
async fn only_admins_can_change_roles() {
    let app = TestApp::new();
    let student = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await;
    let student_id = student.body["user"]["id"].as_str().unwrap().to_string();
    let admin = app
        .register("Admin", "admin@example.com", "correct horse battery")
        .await;
    let admin_id: Uuid = admin.body["user"]["id"].as_str().unwrap().parse().unwrap();
    app.users.set_role(admin_id, Role::Admin).await.unwrap();

    let forbidden = set_role(&app, &student.access_token(), &student_id, "admin").await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);

    let res = set_role(&app, &admin.access_token(), &student_id, "instructor").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["role"], "instructor");
}

#[tokio::test]
async fn changing_the_role_of_an_unknown_user_or_to_an_unknown_role_fails() {
    let app = TestApp::new();
    let admin = app
        .register("Admin", "admin@example.com", "correct horse battery")
        .await;
    let admin_id: Uuid = admin.body["user"]["id"].as_str().unwrap().parse().unwrap();
    app.users.set_role(admin_id, Role::Admin).await.unwrap();
    let token = admin.access_token();

    let unknown_user = set_role(&app, &token, &Uuid::new_v4().to_string(), "instructor").await;
    assert_eq!(unknown_user.status, StatusCode::NOT_FOUND);

    let unknown_role = set_role(&app, &token, &admin_id.to_string(), "superuser").await;
    assert!(unknown_role.status.is_client_error());
}
