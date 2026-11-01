
use axum::http::StatusCode;
use crate::common::TestApp;

#[tokio::test]
async fn register_creates_student_signs_in_and_sends_verification_email() {
    let app = TestApp::new();

    let res = app
        .register("Dara Sok", "Dara@Example.com", "correct horse battery")
        .await;

    assert_eq!(res.status, StatusCode::CREATED);
    assert_eq!(res.body["user"]["name"], "Dara Sok");
    assert_eq!(res.body["user"]["email"], "dara@example.com");
    assert_eq!(res.body["user"]["role"], "student");
    assert_eq!(res.body["user"]["email_verified"], false);
    assert!(res.body["user"].get("password_hash").is_none());
    assert!(!res.access_token().is_empty());

    let cookie = res
        .set_cookie_header("refresh_token")
        .expect("refresh cookie set");
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("Path=/api/v1/auth"));

    let emails = app.mailer.sent.lock().unwrap();
    assert_eq!(emails.len(), 1);
    assert_eq!(emails[0].to, "dara@example.com");
    assert!(
        emails[0]
            .body
            .contains("http://localhost:3000/verify-email?token=")
    );
}

#[tokio::test]
async fn register_rejects_invalid_input() {
    let app = TestApp::new();

    for (name, email, password) in [
        ("", "a@example.com", "long enough pw"),
        ("Dara", "not-an-email", "long enough pw"),
        ("Dara", "a@example.com", "short"),
    ] {
        let res = app.register(name, email, password).await;
        assert_eq!(
            res.status,
            StatusCode::BAD_REQUEST,
            "{name:?} {email:?} {password:?}"
        );
        assert_eq!(res.body["error"]["code"], "bad_request");
    }
    assert!(app.mailer.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn register_rejects_an_email_that_is_already_taken_case_insensitively() {
    let app = TestApp::new();
    app.register("Dara", "dara@example.com", "correct horse battery")
        .await;

    let res = app
        .register("Other", "DARA@example.com", "another password")
        .await;

    assert_eq!(res.status, StatusCode::CONFLICT);
    assert_eq!(res.body["error"]["code"], "conflict");
}

#[tokio::test]
async fn login_with_correct_credentials_starts_a_session() {
    let app = TestApp::new();
    app.register("Dara", "dara@example.com", "correct horse battery")
        .await;

    let res = app
        .login(" Dara@Example.com ", "correct horse battery")
        .await;

    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["user"]["email"], "dara@example.com");
    assert!(!res.access_token().is_empty());
    assert!(res.refresh_cookie().is_some());
}

#[tokio::test]
async fn login_failures_are_indistinguishable() {
    let app = TestApp::new();
    app.register("Dara", "dara@example.com", "correct horse battery")
        .await;

    let wrong_password = app.login("dara@example.com", "wrong password!").await;
    let unknown_email = app
        .login("nobody@example.com", "correct horse battery")
        .await;

    for res in [&wrong_password, &unknown_email] {
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
        assert!(res.refresh_cookie().is_none());
    }
    assert_eq!(wrong_password.body, unknown_email.body);
}

#[tokio::test]
async fn me_returns_the_signed_in_user() {
    let app = TestApp::new();
    let token = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .access_token();

    let res = app.get_authed("/auth/me", &token).await;

    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["email"], "dara@example.com");
    assert_eq!(res.body["email_verified"], false);
}

#[tokio::test]
async fn me_rejects_missing_invalid_and_expired_access_tokens() {
    let app = TestApp::new();
    let token = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .access_token();

    let missing = app
        .request(axum::http::Method::GET, "/auth/me", None, None, None)
        .await;
    assert_eq!(missing.status, StatusCode::UNAUTHORIZED);

    let garbage = app.get_authed("/auth/me", "not.a.jwt").await;
    assert_eq!(garbage.status, StatusCode::UNAUTHORIZED);

    app.clock.advance(chrono::Duration::minutes(14));
    assert_eq!(
        app.get_authed("/auth/me", &token).await.status,
        StatusCode::OK
    );

    app.clock.advance(chrono::Duration::minutes(2));
    assert_eq!(
        app.get_authed("/auth/me", &token).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn verifying_with_the_emailed_token_marks_the_email_verified_once() {
    let app = TestApp::new();
    let token = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .access_token();
    let verification = app.last_verification_token("dara@example.com");

    let res = app
        .post(
            "/auth/verify-email",
            serde_json::json!({ "token": verification }),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["email_verified"], true);
    assert_eq!(
        app.get_authed("/auth/me", &token).await.body["email_verified"],
        true
    );

    let reused = app
        .post(
            "/auth/verify-email",
            serde_json::json!({ "token": verification }),
        )
        .await;
    assert_eq!(reused.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn verification_tokens_expire_and_can_be_resent() {
    let app = TestApp::new();
    app.register("Dara", "dara@example.com", "correct horse battery")
        .await;
    let stale = app.last_verification_token("dara@example.com");

    app.clock.advance(chrono::Duration::hours(25));
    let expired = app
        .post("/auth/verify-email", serde_json::json!({ "token": stale }))
        .await;
    assert_eq!(expired.status, StatusCode::BAD_REQUEST);

    // Access token from registration has expired too; log in again.
    let token = app
        .login("dara@example.com", "correct horse battery")
        .await
        .access_token();
    let resend = app
        .request(
            axum::http::Method::POST,
            "/auth/resend-verification",
            None,
            Some(&token),
            None,
        )
        .await;
    assert_eq!(resend.status, StatusCode::NO_CONTENT);

    let fresh = app.last_verification_token("dara@example.com");
    assert_ne!(fresh, stale);
    let res = app
        .post("/auth/verify-email", serde_json::json!({ "token": fresh }))
        .await;
    assert_eq!(res.status, StatusCode::OK);
}

#[tokio::test]
async fn resend_verification_is_refused_once_already_verified() {
    let app = TestApp::new();
    let token = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .access_token();
    let verification = app.last_verification_token("dara@example.com");
    app.post(
        "/auth/verify-email",
        serde_json::json!({ "token": verification }),
    )
    .await;

    let res = app
        .request(
            axum::http::Method::POST,
            "/auth/resend-verification",
            None,
            Some(&token),
            None,
        )
        .await;

    assert_eq!(res.status, StatusCode::CONFLICT);
}

async fn refresh(app: &TestApp, cookie: &str) -> crate::common::TestResponse {
    app.request(
        axum::http::Method::POST,
        "/auth/refresh",
        None,
        None,
        Some(cookie),
    )
    .await
}

#[tokio::test]
async fn refresh_rotates_the_refresh_token_and_issues_a_new_access_token() {
    let app = TestApp::new();
    let first = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .refresh_cookie()
        .unwrap();

    app.clock.advance(chrono::Duration::minutes(20)); // access token now expired
    let res = refresh(&app, &first).await;

    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["user"]["email"], "dara@example.com");
    let second = res.refresh_cookie().expect("rotated cookie");
    assert_ne!(second, first);
    assert_eq!(
        app.get_authed("/auth/me", &res.access_token()).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn refresh_without_a_cookie_or_with_an_unknown_one_is_unauthorized() {
    let app = TestApp::new();
    let none = app
        .request(axum::http::Method::POST, "/auth/refresh", None, None, None)
        .await;
    assert_eq!(none.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        refresh(&app, "made-up").await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn reusing_a_rotated_refresh_token_revokes_every_session_of_that_user() {
    let app = TestApp::new();
    let stolen = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .refresh_cookie()
        .unwrap();
    let legit = refresh(&app, &stolen).await.refresh_cookie().unwrap();

    let replay = refresh(&app, &stolen).await;
    assert_eq!(replay.status, StatusCode::UNAUTHORIZED);

    // The legitimate, newer token was revoked as well.
    assert_eq!(refresh(&app, &legit).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_tokens_expire() {
    let app = TestApp::new();
    let cookie = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .refresh_cookie()
        .unwrap();

    app.clock.advance(chrono::Duration::days(31));

    assert_eq!(
        refresh(&app, &cookie).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn logout_revokes_the_refresh_token_and_clears_the_cookie() {
    let app = TestApp::new();
    let cookie = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .refresh_cookie()
        .unwrap();

    let res = app
        .request(
            axum::http::Method::POST,
            "/auth/logout",
            None,
            None,
            Some(&cookie),
        )
        .await;

    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let cleared = res
        .set_cookie_header("refresh_token")
        .expect("cookie cleared");
    assert!(cleared.contains("Max-Age=0"));
    assert_eq!(
        refresh(&app, &cookie).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn logout_without_a_session_still_succeeds() {
    let app = TestApp::new();
    let res = app
        .request(axum::http::Method::POST, "/auth/logout", None, None, None)
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn registration_succeeds_even_if_the_verification_email_cannot_be_sent() {
    let app = TestApp::new();
    app.mailer.set_failing(true);

    let res = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await;

    assert_eq!(res.status, StatusCode::CREATED);
    assert!(res.refresh_cookie().is_some());
}

#[tokio::test]
async fn replaying_a_logged_out_refresh_token_does_not_end_other_sessions() {
    let app = TestApp::new();
    let laptop = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .refresh_cookie()
        .unwrap();
    let phone = app
        .login("dara@example.com", "correct horse battery")
        .await
        .refresh_cookie()
        .unwrap();

    app.request(
        axum::http::Method::POST,
        "/auth/logout",
        None,
        None,
        Some(&laptop),
    )
    .await;
    assert_eq!(
        refresh(&app, &laptop).await.status,
        StatusCode::UNAUTHORIZED
    );

    assert_eq!(refresh(&app, &phone).await.status, StatusCode::OK);
}

async fn resend(app: &TestApp, token: &str) -> crate::common::TestResponse {
    app.request(
        axum::http::Method::POST,
        "/auth/resend-verification",
        None,
        Some(token),
        None,
    )
    .await
}

#[tokio::test]
async fn resending_verification_is_throttled_and_invalidates_older_links() {
    let app = TestApp::new();
    let token = app
        .register("Dara", "dara@example.com", "correct horse battery")
        .await
        .access_token();
    let first = app.last_verification_token("dara@example.com");

    let too_soon = resend(&app, &token).await;
    assert_eq!(too_soon.status, StatusCode::TOO_MANY_REQUESTS);

    app.clock.advance(chrono::Duration::seconds(61));
    assert_eq!(resend(&app, &token).await.status, StatusCode::NO_CONTENT);
    let second = app.last_verification_token("dara@example.com");

    let old = app
        .post("/auth/verify-email", serde_json::json!({ "token": first }))
        .await;
    assert_eq!(old.status, StatusCode::BAD_REQUEST);
    let new = app
        .post("/auth/verify-email", serde_json::json!({ "token": second }))
        .await;
    assert_eq!(new.status, StatusCode::OK);
}
