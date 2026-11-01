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
    config::{Config, S3Config, SmtpConfig},
    error::{AppError, AppResult},
    infra::{
        clock::Clock,
        health::HealthProbe,
        mailer::{Email, Mailer},
        storage::Storage,
    },
    modules::{
        categories::{Category, CategoryRepo},
        courses::{model::Course, repo::CourseRepo},
        curriculum::{
            model::{Lecture, Section},
            repo::{CurriculumRepo, SectionOrder},
        },
        instructors::{InstructorProfile, InstructorProfileRepo},
        users::{
            model::{NewUser, Role, User},
            repo::UserRepo,
        },
        videos::{
            model::{VideoAsset, VideoStatus},
            repo::VideoAssetRepo,
        },
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

pub struct MemCategories(Vec<Category>);

impl Default for MemCategories {
    fn default() -> Self {
        let cat = |name: &str, slug: &str| Category {
            id: Uuid::new_v4(),
            parent_id: None,
            name: name.into(),
            slug: slug.into(),
        };
        Self(vec![
            cat("Web Development", "web-development"),
            cat("Data Science", "data-science"),
        ])
    }
}

#[async_trait]
impl CategoryRepo for MemCategories {
    async fn list(&self) -> AppResult<Vec<Category>> {
        Ok(self.0.clone())
    }
    async fn find(&self, id: Uuid) -> AppResult<Option<Category>> {
        Ok(self.0.iter().find(|c| c.id == id).cloned())
    }
}

#[derive(Default)]
pub struct MemInstructorProfiles(Mutex<Vec<InstructorProfile>>);

#[async_trait]
impl InstructorProfileRepo for MemInstructorProfiles {
    async fn find(&self, user_id: Uuid) -> AppResult<Option<InstructorProfile>> {
        let profiles = self.0.lock().unwrap();
        Ok(profiles.iter().find(|p| p.user_id == user_id).cloned())
    }
    async fn upsert(&self, profile: InstructorProfile) -> AppResult<InstructorProfile> {
        let mut profiles = self.0.lock().unwrap();
        profiles.retain(|p| p.user_id != profile.user_id);
        profiles.push(profile.clone());
        Ok(profile)
    }
}

#[derive(Default)]
pub struct MemCourses(Mutex<Vec<Course>>);

#[async_trait]
impl CourseRepo for MemCourses {
    async fn insert(&self, course: &Course) -> AppResult<()> {
        self.0.lock().unwrap().push(course.clone());
        Ok(())
    }
    async fn find(&self, id: Uuid) -> AppResult<Option<Course>> {
        Ok(self.0.lock().unwrap().iter().find(|c| c.id == id).cloned())
    }
    async fn list_by_instructor(&self, instructor_id: Uuid) -> AppResult<Vec<Course>> {
        let courses = self.0.lock().unwrap();
        let mut mine: Vec<_> = courses
            .iter()
            .filter(|c| c.instructor_id == instructor_id)
            .cloned()
            .collect();
        mine.sort_by_key(|c| std::cmp::Reverse(c.created_at));
        Ok(mine)
    }
    async fn save(&self, course: &Course) -> AppResult<()> {
        let mut courses = self.0.lock().unwrap();
        if let Some(existing) = courses.iter_mut().find(|c| c.id == course.id) {
            *existing = course.clone();
        }
        Ok(())
    }
    async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.0.lock().unwrap().retain(|c| c.id != id);
        Ok(())
    }
}

#[derive(Default)]
pub struct MemCurriculum {
    sections: Mutex<Vec<Section>>,
    lectures: Mutex<Vec<Lecture>>,
}

#[async_trait]
impl CurriculumRepo for MemCurriculum {
    async fn sections(&self, course_id: Uuid) -> AppResult<Vec<Section>> {
        let sections = self.sections.lock().unwrap();
        Ok(sections
            .iter()
            .filter(|s| s.course_id == course_id)
            .cloned()
            .collect())
    }
    async fn lectures(&self, course_id: Uuid) -> AppResult<Vec<Lecture>> {
        let lectures = self.lectures.lock().unwrap();
        Ok(lectures
            .iter()
            .filter(|l| l.course_id == course_id)
            .cloned()
            .collect())
    }
    async fn find_section(&self, id: Uuid) -> AppResult<Option<Section>> {
        Ok(self
            .sections
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned())
    }
    async fn find_lecture(&self, id: Uuid) -> AppResult<Option<Lecture>> {
        Ok(self
            .lectures
            .lock()
            .unwrap()
            .iter()
            .find(|l| l.id == id)
            .cloned())
    }
    async fn insert_section(&self, section: &Section) -> AppResult<()> {
        self.sections.lock().unwrap().push(section.clone());
        Ok(())
    }
    async fn save_section(&self, section: &Section) -> AppResult<()> {
        for s in self
            .sections
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|s| s.id == section.id)
        {
            *s = section.clone();
        }
        Ok(())
    }
    async fn delete_section(&self, id: Uuid) -> AppResult<()> {
        self.sections.lock().unwrap().retain(|s| s.id != id);
        self.lectures.lock().unwrap().retain(|l| l.section_id != id);
        Ok(())
    }
    async fn insert_lecture(&self, lecture: &Lecture) -> AppResult<()> {
        self.lectures.lock().unwrap().push(lecture.clone());
        Ok(())
    }
    async fn save_lecture(&self, lecture: &Lecture) -> AppResult<()> {
        for l in self
            .lectures
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|l| l.id == lecture.id)
        {
            *l = lecture.clone();
        }
        Ok(())
    }
    async fn delete_lecture(&self, id: Uuid) -> AppResult<()> {
        self.lectures.lock().unwrap().retain(|l| l.id != id);
        Ok(())
    }
    async fn reorder(&self, _course_id: Uuid, order: &[SectionOrder]) -> AppResult<()> {
        let mut sections = self.sections.lock().unwrap();
        let mut lectures = self.lectures.lock().unwrap();
        for (s_pos, so) in order.iter().enumerate() {
            if let Some(s) = sections.iter_mut().find(|s| s.id == so.section_id) {
                s.position = s_pos as i32;
            }
            for (l_pos, id) in so.lecture_ids.iter().enumerate() {
                if let Some(l) = lectures.iter_mut().find(|l| l.id == *id) {
                    l.section_id = so.section_id;
                    l.position = l_pos as i32;
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct MemVideoAssets(pub Mutex<Vec<VideoAsset>>);

#[async_trait]
impl VideoAssetRepo for MemVideoAssets {
    async fn insert(&self, asset: &VideoAsset) -> AppResult<()> {
        self.0.lock().unwrap().push(asset.clone());
        Ok(())
    }
    async fn find(&self, id: Uuid) -> AppResult<Option<VideoAsset>> {
        Ok(self.0.lock().unwrap().iter().find(|a| a.id == id).cloned())
    }
    async fn save(&self, asset: &VideoAsset) -> AppResult<()> {
        for a in self
            .0
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|a| a.id == asset.id)
        {
            *a = asset.clone();
        }
        Ok(())
    }
    async fn claim_next(
        &self,
        now: DateTime<Utc>,
        stale_before: DateTime<Utc>,
    ) -> AppResult<Option<VideoAsset>> {
        let mut assets = self.0.lock().unwrap();
        let mut queue: Vec<_> = assets
            .iter_mut()
            .filter(|a| {
                a.status == VideoStatus::Processing && a.claimed_at.is_none_or(|c| c < stale_before)
            })
            .collect();
        queue.sort_by_key(|a| a.created_at);
        Ok(queue.into_iter().next().map(|a| {
            a.claimed_at = Some(now);
            a.attempts += 1;
            a.clone()
        }))
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
    pub categories: Arc<MemCategories>,
    pub video_assets: Arc<MemVideoAssets>,
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
        s3: S3Config {
            endpoint: "http://localhost:9000".into(),
            region: "us-east-1".into(),
            bucket: "reanit-media".into(),
            access_key: "test-access".into(),
            secret_key: "test-secret".into(),
            public_url: "http://cdn.test/reanit-media".into(),
            path_style: true,
        },
    }
}

impl TestApp {
    pub fn new() -> Self {
        let config = test_config();
        let storage = Arc::new(Storage::new(config.s3.clone()));
        let users = Arc::new(MemUsers::default());
        let categories = Arc::new(MemCategories::default());
        let video_assets = Arc::new(MemVideoAssets::default());
        let mailer = Arc::new(CapturingMailer::default());
        let clock = Arc::new(FakeClock(Mutex::new(
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        )));
        let state = AppState {
            config: Arc::new(config),
            users: users.clone(),
            categories: categories.clone(),
            instructor_profiles: Arc::new(MemInstructorProfiles::default()),
            courses: Arc::new(MemCourses::default()),
            curriculum: Arc::new(MemCurriculum::default()),
            video_assets: video_assets.clone(),
            refresh_tokens: Arc::new(MemRefreshTokens::default()),
            verification_tokens: Arc::new(MemVerificationTokens::default()),
            mailer: mailer.clone(),
            clock: clock.clone(),
            health: Arc::new(OkProbe),
            storage,
        };
        Self {
            router: routes::build(state),
            users,
            categories,
            video_assets,
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

    pub async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: &str,
    ) -> TestResponse {
        self.request(method, path, body, Some(token), None).await
    }

    /// Registers and verifies a student; returns their access token.
    pub async fn verified_student(&self, email: &str) -> String {
        let token = self
            .register("Dara Sok", email, "correct horse battery")
            .await
            .access_token();
        let verification = self.last_verification_token(email);
        self.post(
            "/auth/verify-email",
            serde_json::json!({ "token": verification }),
        )
        .await;
        token
    }

    /// A verified user who has become an instructor; returns their access token.
    pub async fn instructor(&self, email: &str) -> String {
        let token = self.verified_student(email).await;
        let res = self
            .send(
                Method::POST,
                "/me/become-instructor",
                Some(serde_json::json!({ "headline": "Instructor" })),
                &token,
            )
            .await;
        assert_eq!(
            res.status,
            StatusCode::OK,
            "become instructor: {:?}",
            res.body
        );
        token
    }

    /// A verified admin; returns their access token.
    pub async fn admin(&self, email: &str) -> String {
        let token = self.verified_student(email).await;
        let user = self.users.find_by_email(email).await.unwrap().unwrap();
        self.users.set_role(user.id, Role::Admin).await.unwrap();
        token
    }

    pub fn category_id(&self) -> String {
        self.categories.0[0].id.to_string()
    }

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
