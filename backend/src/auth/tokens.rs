use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    iat: i64,
    exp: i64,
}

pub fn issue_access_token(
    secret: &str,
    user_id: Uuid,
    now: DateTime<Utc>,
    ttl: Duration,
) -> AppResult<String> {
    let claims = Claims {
        sub: user_id,
        iat: now.timestamp(),
        exp: (now + ttl).timestamp(),
    };
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| anyhow::anyhow!("failed to sign access token: {e}").into())
}

/// Returns the user id if the token is validly signed and not expired at `now`.
/// Expiry is checked against the injected clock rather than the system clock.
pub fn verify_access_token(secret: &str, token: &str, now: DateTime<Utc>) -> AppResult<Uuid> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false;
    validation.required_spec_claims.clear();
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| AppError::Unauthorized)?;
    if data.claims.exp <= now.timestamp() {
        return Err(AppError::Unauthorized);
    }
    Ok(data.claims.sub)
}

/// A random, URL-safe opaque token (256 bits).
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Opaque tokens are stored hashed so a database leak doesn't leak live tokens.
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
