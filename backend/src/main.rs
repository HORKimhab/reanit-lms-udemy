use std::sync::Arc;

use anyhow::Context;
use reanit_api::{
    config::Config,
    infra::{clock::SystemClock, health::PgRedisProbe, mailer::SmtpMailer, postgres::PgStore},
    routes,
    state::AppState,
};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "reanit_api=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;

    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .context("failed to connect to Postgres")?;

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .context("failed to run migrations")?;

    let redis_client = redis::Client::open(config.redis_url.as_str())?;
    let redis = redis::aio::ConnectionManager::new(redis_client)
        .await
        .context("failed to connect to Redis")?;

    let store = Arc::new(PgStore::new(db.clone()));
    let mailer = Arc::new(SmtpMailer::new(&config.smtp)?);
    let addr = format!("{}:{}", config.host, config.port);

    let state = AppState {
        config: Arc::new(config),
        users: store.clone(),
        categories: store.clone(),
        instructor_profiles: store.clone(),
        courses: store.clone(),
        refresh_tokens: store.clone(),
        verification_tokens: store,
        mailer,
        clock: Arc::new(SystemClock),
        health: Arc::new(PgRedisProbe { db, redis }),
    };
    let app = routes::build(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("API listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("shutting down");
}
