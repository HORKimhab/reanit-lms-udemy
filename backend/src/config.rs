use anyhow::Context;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub redis_url: String,
    pub cors_origin: String,
    /// Base URL of the web app, used to build links in emails.
    pub frontend_url: String,
    pub jwt_secret: String,
    pub access_token_ttl_minutes: i64,
    pub refresh_token_ttl_days: i64,
    pub email_verification_ttl_hours: i64,
    /// Set the `Secure` flag on auth cookies (true in production / HTTPS).
    pub cookie_secure: bool,
    pub smtp: SmtpConfig,
    pub s3: S3Config,
}

#[derive(Clone, Debug)]
pub struct S3Config {
    /// e.g. `http://localhost:9000` (MinIO) or `https://<account>.r2.cloudflarestorage.com`.
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    /// Base URL for publicly readable objects, e.g. `http://localhost:9000/reanit-media`.
    pub public_url: String,
    /// `endpoint/bucket/key` (MinIO, R2) instead of `bucket.endpoint/key`.
    pub path_style: bool,
}

#[derive(Clone, Debug)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: String,
}

fn var(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("{key} must be set"))
}

fn var_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}

fn parse_or<T: std::str::FromStr>(key: &str, default: T) -> anyhow::Result<T> {
    match std::env::var(key) {
        Ok(v) => v.parse().ok().with_context(|| format!("{key} is invalid")),
        Err(_) => Ok(default),
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let jwt_secret = var("JWT_SECRET")?;
        anyhow::ensure!(
            jwt_secret.len() >= 32,
            "JWT_SECRET must be at least 32 characters"
        );

        Ok(Self {
            host: var_or("APP_HOST", "0.0.0.0"),
            port: parse_or("APP_PORT", 8080)?,
            database_url: var("DATABASE_URL")?,
            redis_url: var("REDIS_URL")?,
            cors_origin: var_or("CORS_ORIGIN", "http://localhost:3000"),
            frontend_url: var_or("FRONTEND_URL", "http://localhost:3000"),
            jwt_secret,
            access_token_ttl_minutes: parse_or("ACCESS_TOKEN_TTL_MINUTES", 15)?,
            refresh_token_ttl_days: parse_or("REFRESH_TOKEN_TTL_DAYS", 30)?,
            email_verification_ttl_hours: parse_or("EMAIL_VERIFICATION_TTL_HOURS", 24)?,
            cookie_secure: parse_or("COOKIE_SECURE", false)?,
            smtp: SmtpConfig {
                host: var_or("SMTP_HOST", "localhost"),
                port: parse_or("SMTP_PORT", 1025)?,
                username: std::env::var("SMTP_USERNAME").ok(),
                password: std::env::var("SMTP_PASSWORD").ok(),
                from: var_or("MAIL_FROM", "ReanIT <no-reply@reanit.local>"),
            },
            s3: S3Config {
                endpoint: var_or("S3_ENDPOINT", "http://localhost:9000"),
                region: var_or("S3_REGION", "us-east-1"),
                bucket: var_or("S3_BUCKET", "reanit-media"),
                access_key: var("S3_ACCESS_KEY")?,
                secret_key: var("S3_SECRET_KEY")?,
                public_url: var_or("S3_PUBLIC_URL", "http://localhost:9000/reanit-media"),
                path_style: parse_or("S3_PATH_STYLE", true)?,
            },
        })
    }
}
