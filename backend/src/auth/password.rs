use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

use crate::error::AppResult;

/// Hashes with Argon2id on a blocking thread (it is deliberately CPU-heavy).
pub async fn hash(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(Into::into)
}

pub async fn verify(password: String, password_hash: String) -> AppResult<bool> {
    tokio::task::spawn_blocking(move || verify_sync(&password, &password_hash))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(Into::into)
}

/// A real Argon2 hash of a random password, so failed logins for unknown
/// emails cost the same as for known ones (no user-enumeration timing leak).
static DUMMY_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    Argon2::default()
        .hash_password(crate::auth::tokens::random_token().as_bytes())
        .expect("hashing a random password")
        .to_string()
});

/// Burns the same CPU as `verify`, always returning false.
pub async fn verify_dummy(password: String) -> AppResult<bool> {
    tokio::task::spawn_blocking(move || {
        let _ = verify_sync(&password, &DUMMY_HASH);
    })
    .await
    .map_err(anyhow::Error::from)?;
    Ok(false)
}

fn verify_sync(password: &str, password_hash: &str) -> anyhow::Result<bool> {
    let parsed = PasswordHash::new(password_hash)
        .map_err(|e| anyhow::anyhow!("invalid stored password hash: {e}"))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
