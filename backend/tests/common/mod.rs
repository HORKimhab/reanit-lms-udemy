//! In-memory adapters + a `TestApp` that drives the real router in-process.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use http_body_util::BodyExt;
use reanit_api::{
    auth::store::{RefreshToken, RefreshTokenRepo, VerificationTokenRepo},
    config::{Config, SmtpConfig},
    error::{AppError, AppResult},
    infra::{
        clock::Clock,
        health::HealthProbe,
        mailer::{Email, Mailer},
    },
    modules::users::{
        model::{NewUser, Role, User},
        repo::UserRepo,
    },
    routes,
    state::AppState,
};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

// ---------- in-memory adapters ----------

#[derive(Default)]
pub struct MemUsers(Mutex<Vec<User>>);

#[async_trait]
impl UserRepo for MemUsers {
    async fn create(&self, new: NewUser, now: DateTime<Utc>) -> AppResult<User> {
        let mut users = self.0.lock().unwrap();
        if users
            .iter()
            .any(|u| u.email.eq_ignore_ascii_case(&new.email))
        {
            return Err(AppError::Conflict("email is already registered".into()));
        }
        let user = User {
            id: Uuid::new_v4(),
            name: new.name,
            email: new.email,
            password_hash: new.password_hash,
            role: Role::Student,
            email_verified_at: None,
            avatar_url: None,
            created_at: now,
        };
        users.push(user.clone());
        Ok(user)
    }
    async fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.email.eq_ignore_ascii_case(email))
            .cloned())
    }
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<User>> {
        Ok(self.0.lock().unwrap().iter().find(|u| u.id == id).cloned())
    }
    async fn mark_email_verified(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<Option<User>> {
        let mut users = self.0.lock().unwrap();
        Ok(users.iter_mut().find(|u| u.id == id).map(|u| {
            u.email_verified_at.get_or_insert(at);
            u.clone()
        }))
    }
    async fn set_role(&self, id: Uuid, role: Role) -> AppResult<Option<User>> {
        let mut users = self.0.lock().unwrap();
        Ok(users.iter_mut().find(|u| u.id == id).map(|u| {
            u.role = role;
            u.clone()
        }))
    }
}

#[derive(Default)]
pub struct MemRefreshTokens(Mutex<Vec<(String, RefreshToken)>>);

#[async_trait]
impl RefreshTokenRepo for MemRefreshTokens {
    async fn insert(
        &self,
        user_id: Uuid,
        hash: &str,
        expires_at: DateTime<Utc>,
        _now: DateTime<Utc>,
    ) -> AppResult<()> {
        let token = RefreshToken {
            id: Uuid::new_v4(),
            user_id,
            expires_at,
            revoked_at: None,
        };
        self.0.lock().unwrap().push((hash.to_string(), token));
        Ok(())
    }
    async fn find_by_hash(&self, hash: &str) -> AppResult<Option<RefreshToken>> {
        let tokens = self.0.lock().unwrap();
        Ok(tokens
            .iter()
            .find(|(h, _)| h == hash)
            .map(|(_, t)| t.clone()))
    }
    async fn revoke_if_active(&self, id: Uuid, at: DateTime<Utc>) -> AppResult<bool> {
        let mut tokens = self.0.lock().unwrap();
        match tokens.iter_mut().find(|(_, t)| t.id == id) {
            Some((_, t)) if t.revoked_at.is_none() => {
                t.revoked_at = Some(at);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    async fn revoke_all_for_user(&self, user_id: Uuid, at: DateTime<Utc>) -> AppResult<()> {
        for (_, t) in self.0.lock().unwrap().iter_mut() {
            if t.user_id == user_id && t.revoked_at.is_none() {
                t.revoked_at = Some(at);
            }
        }
        Ok(())
    }
    async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.0.lock().unwrap().retain(|(_, t)| t.id != id);
        Ok(())
    }
}

struct MemVerification {
    hash: String,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    used: bool,
}

#[derive(Default)]
pub struct MemVerificationTokens(Mutex<Vec<MemVerification>>);

#[async_trait]
impl VerificationTokenRepo for MemVerificationTokens {
    async fn replace_for_user(
        &self,
        user_id: Uuid,
        hash: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        let mut tokens = self.0.lock().unwrap();
        tokens.retain(|t| t.user_id != user_id || t.used);
        tokens.push(MemVerification {
            hash: hash.into(),
            user_id,
            expires_at,
            created_at: now,
            used: false,
        });
        Ok(())
    }
    async fn last_issued_at(&self, user_id: Uuid) -> AppResult<Option<DateTime<Utc>>> {
        let tokens = self.0.lock().unwrap();
        Ok(tokens
            .iter()
            .filter(|t| t.user_id == user_id)
            .map(|t| t.created_at)
            .max())
    }
    async fn consume(&self, hash: &str, now: DateTime<Utc>) -> AppResult<Option<Uuid>> {
        let mut tokens = self.0.lock().unwrap();
        Ok(tokens
            .iter_mut()
            .find(|t| t.hash == hash && !t.used && t.expires_at > now)
            .map(|t| {
                t.used = true;
                t.user_id
            }))
    }
}

#[derive(Default)]
pub struct CapturingMailer {
    pub sent: Mutex<Vec<Email>>,
    /// When set, every send fails (simulates an SMTP outage).
    pub failing: std::sync::atomic::AtomicBool,
}

impl CapturingMailer {
    pub fn set_failing(&self, failing: bool) {
        self.failing
            .store(failing, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait]
impl Mailer for CapturingMailer {
    async fn send(&self, email: Email) -> AppResult<()> {
        if self.failing.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(anyhow::anyhow!("smtp unavailable").into());
        }
        self.sent.lock().unwrap().push(email);
        Ok(())
    }
}

pub struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    pub fn advance(&self, by: Duration) {
        *self.0.lock().unwrap() += by;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

struct OkProbe;

#[async_trait]
impl HealthProbe for OkProbe {
    async fn check(&self) -> AppResult<()> {
        Ok(())
    }
}

// ---------- test app ----------

pub struct TestApp {
    router: Router,
    pub users: Arc<MemUsers>,
    pub mailer: Arc<CapturingMailer>,
    pub clock: Arc<FakeClock>,
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Value,
}

impl TestResponse {
    /// Value of the `refresh_token` cookie set by this response, if any.
    pub fn refresh_cookie(&self) -> Option<String> {
        self.set_cookie_header("refresh_token").and_then(|c| {
            let value = c.split(';').next()?.split_once('=')?.1.to_string();
            (!value.is_empty()).then_some(value)
        })
    }

    pub fn set_cookie_header(&self, name: &str) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|c| c.starts_with(&format!("{name}=")))
            .map(String::from)
    }

    pub fn access_token(&self) -> String {
        self.body["access_token"]
            .as_str()
            .expect("no access_token in body")
            .to_string()
    }
}

pub fn test_config() -> Config {
    Config {
        host: "127.0.0.1".into(),
        port: 0,
        database_url: String::new(),
        redis_url: String::new(),
        cors_origin: "http://localhost:3000".into(),
        frontend_url: "http://localhost:3000".into(),
        jwt_secret: "test-secret-test-secret-test-secret!!".into(),
        access_token_ttl_minutes: 15,
        refresh_token_ttl_days: 30,
        email_verification_ttl_hours: 24,
        cookie_secure: false,
        smtp: SmtpConfig {
            host: String::new(),
            port: 0,
            username: None,
            password: None,
            from: "test@example.com".into(),
        },
    }
}

impl TestApp {
    pub fn new() -> Self {
        let users = Arc::new(MemUsers::default());
        let mailer = Arc::new(CapturingMailer::default());
        let clock = Arc::new(FakeClock(Mutex::new(
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        )));
        let state = AppState {
            config: Arc::new(test_config()),
            users: users.clone(),
            refresh_tokens: Arc::new(MemRefreshTokens::default()),
            verification_tokens: Arc::new(MemVerificationTokens::default()),
            mailer: mailer.clone(),
            clock: clock.clone(),
            health: Arc::new(OkProbe),
        };
        Self {
            router: routes::build(state),
            users,
            mailer,
            clock,
        }
    }

    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        bearer: Option<&str>,
        refresh_cookie: Option<&str>,
    ) -> TestResponse {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"));
        if let Some(token) = bearer {
            req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        if let Some(cookie) = refresh_cookie {
            req = req.header(header::COOKIE, format!("refresh_token={cookie}"));
        }
        let req = match body {
            Some(json) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();

        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        TestResponse {
            status,
            headers,
            body,
        }
    }

    pub async fn post(&self, path: &str, body: Value) -> TestResponse {
        self.request(Method::POST, path, Some(body), None, None)
            .await
    }

    pub async fn get_authed(&self, path: &str, token: &str) -> TestResponse {
        self.request(Method::GET, path, None, Some(token), None)
            .await
    }

    pub async fn register(&self, name: &str, email: &str, password: &str) -> TestResponse {
        self.post(
            "/auth/register",
            serde_json::json!({ "name": name, "email": email, "password": password }),
        )
        .await
    }

    pub async fn login(&self, email: &str, password: &str) -> TestResponse {
        self.post(
            "/auth/login",
            serde_json::json!({ "email": email, "password": password }),
        )
        .await
    }

    /// The verification token from the last email sent to `to`.
    pub fn last_verification_token(&self, to: &str) -> String {
        let emails = self.mailer.sent.lock().unwrap();
        let email = emails
            .iter()
            .rev()
            .find(|e| e.to == to)
            .expect("no email sent");
        let start = email.body.find("token=").expect("no token in email") + "token=".len();
        email.body[start..]
            .split_whitespace()
            .next()
            .unwrap()
            .to_string()
    }
}
