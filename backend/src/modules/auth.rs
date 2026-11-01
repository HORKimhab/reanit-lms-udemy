use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::{
    auth::{extractor::CurrentUser, password, tokens},
    error::{AppError, AppResult},
    extract::ApiJson,
    infra::mailer::Email,
    modules::users::model::{NewUser, User, UserDto},
    state::AppState,
};

const REFRESH_COOKIE: &str = "refresh_token";
const REFRESH_COOKIE_PATH: &str = "/api/v1/auth";
const MIN_PASSWORD_LEN: usize = 8;
const MAX_PASSWORD_LEN: usize = 128;
const RESEND_COOLDOWN_SECS: i64 = 60;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/verify-email", post(verify_email))
        .route("/resend-verification", post(resend_verification))
}

#[derive(Deserialize)]
struct RegisterRequest {
    name: String,
    email: String,
    password: String,
}

#[derive(Serialize)]
struct SessionResponse {
    user: UserDto,
    access_token: String,
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiJson(req): ApiJson<RegisterRequest>,
) -> AppResult<(StatusCode, CookieJar, Json<SessionResponse>)> {
    let name = req.name.trim().to_string();
    let email = normalize_email(&req.email);
    validate_registration(&name, &email, &req.password)?;

    let password_hash = password::hash(req.password).await?;
    let user = state
        .users
        .create(
            NewUser {
                name,
                email,
                password_hash,
            },
            state.clock.now(),
        )
        .await?;

    // The account exists either way; a mail outage must not turn sign-up into a
    // 500 that the client then retries into a 409. The user can resend later.
    if let Err(err) = send_verification_email(&state, &user).await {
        tracing::error!(user_id = %user.id, error = ?err, "failed to send verification email");
    }
    let (jar, body) = start_session(&state, jar, &user).await?;
    Ok((StatusCode::CREATED, jar, Json(body)))
}

fn validate_registration(name: &str, email: &str, password: &str) -> AppResult<()> {
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::BadRequest("name must be 1-100 characters".into()));
    }
    if !is_plausible_email(email) {
        return Err(AppError::BadRequest("email is invalid".into()));
    }
    let len = password.chars().count();
    if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&len) {
        return Err(AppError::BadRequest(format!(
            "password must be {MIN_PASSWORD_LEN}-{MAX_PASSWORD_LEN} characters"
        )));
    }
    Ok(())
}

/// Deliberately loose: real validation happens when the user clicks the
/// verification link.
fn is_plausible_email(email: &str) -> bool {
    match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !email.chars().any(char::is_whitespace)
                && email.len() <= 254
        }
        None => false,
    }
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiJson(req): ApiJson<LoginRequest>,
) -> AppResult<(CookieJar, Json<SessionResponse>)> {
    let user = state
        .users
        .find_by_email(&normalize_email(&req.email))
        .await?;
    let valid = match &user {
        Some(u) => password::verify(req.password, u.password_hash.clone()).await?,
        None => password::verify_dummy(req.password).await?,
    };
    let user = match user {
        Some(u) if valid => u,
        _ => return Err(AppError::Unauthorized),
    };
    let (jar, body) = start_session(&state, jar, &user).await?;
    Ok((jar, Json(body)))
}

async fn me(CurrentUser(user): CurrentUser) -> Json<UserDto> {
    Json((&user).into())
}

#[derive(Deserialize)]
struct VerifyEmailRequest {
    token: String,
}

async fn verify_email(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<VerifyEmailRequest>,
) -> AppResult<Json<UserDto>> {
    let now = state.clock.now();
    let invalid = || AppError::BadRequest("verification link is invalid or has expired".into());

    let user_id = state
        .verification_tokens
        .consume(&tokens::hash_token(&req.token), now)
        .await?
        .ok_or_else(invalid)?;
    let user = state
        .users
        .mark_email_verified(user_id, now)
        .await?
        .ok_or_else(invalid)?;
    Ok(Json((&user).into()))
}

async fn resend_verification(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<StatusCode> {
    if user.is_email_verified() {
        return Err(AppError::Conflict("email is already verified".into()));
    }
    let last = state.verification_tokens.last_issued_at(user.id).await?;
    if last.is_some_and(|at| state.clock.now() - at < Duration::seconds(RESEND_COOLDOWN_SECS)) {
        return Err(AppError::TooManyRequests(format!(
            "please wait {RESEND_COOLDOWN_SECS} seconds before requesting another email"
        )));
    }
    send_verification_email(&state, &user).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<(CookieJar, Json<SessionResponse>)> {
    let now = state.clock.now();
    let presented = jar
        .get(REFRESH_COOKIE)
        .ok_or(AppError::Unauthorized)?
        .value()
        .to_string();
    let token = state
        .refresh_tokens
        .find_by_hash(&tokens::hash_token(&presented))
        .await?
        .ok_or(AppError::Unauthorized)?;

    if token.revoked_at.is_some() {
        // A rotated token was replayed: assume it was stolen and end every session.
        tracing::warn!(user_id = %token.user_id, "refresh token reuse detected");
        state
            .refresh_tokens
            .revoke_all_for_user(token.user_id, now)
            .await?;
        return Err(AppError::Unauthorized);
    }
    if token.expires_at <= now {
        return Err(AppError::Unauthorized);
    }
    // Lost a race with a concurrent refresh using the same token.
    if !state.refresh_tokens.revoke_if_active(token.id, now).await? {
        return Err(AppError::Unauthorized);
    }

    let user = state
        .users
        .find_by_id(token.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let (jar, body) = start_session(&state, jar, &user).await?;
    Ok((jar, Json(body)))
}

/// Idempotent: always clears the cookie, deleting the token if it is known.
async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<(CookieJar, StatusCode)> {
    if let Some(cookie) = jar.get(REFRESH_COOKIE) {
        let hash = tokens::hash_token(cookie.value());
        if let Some(token) = state.refresh_tokens.find_by_hash(&hash).await? {
            state.refresh_tokens.delete(token.id).await?;
        }
    }
    let jar = jar.remove(Cookie::build(REFRESH_COOKIE).path(REFRESH_COOKIE_PATH));
    Ok((jar, StatusCode::NO_CONTENT))
}

/// Issues an access token plus a rotating refresh token (httpOnly cookie).
async fn start_session(
    state: &AppState,
    jar: CookieJar,
    user: &User,
) -> AppResult<(CookieJar, SessionResponse)> {
    let now = state.clock.now();
    let cfg = &state.config;

    let access_token = tokens::issue_access_token(
        &cfg.jwt_secret,
        user.id,
        now,
        Duration::minutes(cfg.access_token_ttl_minutes),
    )?;

    let refresh_token = tokens::random_token();
    let refresh_ttl = Duration::days(cfg.refresh_token_ttl_days);
    state
        .refresh_tokens
        .insert(
            user.id,
            &tokens::hash_token(&refresh_token),
            now + refresh_ttl,
            now,
        )
        .await?;

    let cookie = Cookie::build((REFRESH_COOKIE, refresh_token))
        .http_only(true)
        .secure(cfg.cookie_secure)
        .same_site(SameSite::Lax)
        .path(REFRESH_COOKIE_PATH)
        .max_age(time_duration(refresh_ttl))
        .build();

    Ok((
        jar.add(cookie),
        SessionResponse {
            user: user.into(),
            access_token,
        },
    ))
}

async fn send_verification_email(state: &AppState, user: &User) -> AppResult<()> {
    let now = state.clock.now();
    let token = tokens::random_token();
    let expires_at = now + Duration::hours(state.config.email_verification_ttl_hours);
    state
        .verification_tokens
        .replace_for_user(user.id, &tokens::hash_token(&token), expires_at, now)
        .await?;

    let link = format!("{}/verify-email?token={token}", state.config.frontend_url);
    state
        .mailer
        .send(Email {
            to: user.email.clone(),
            subject: "Verify your ReanIT email".into(),
            body: format!(
                "Hi {},\n\nWelcome to ReanIT! Please confirm your email address:\n\n{link}\n\n\
                 This link expires in {} hours.\n",
                user.name, state.config.email_verification_ttl_hours
            ),
        })
        .await
}

fn time_duration(d: Duration) -> cookie::time::Duration {
    cookie::time::Duration::seconds(d.num_seconds())
}
