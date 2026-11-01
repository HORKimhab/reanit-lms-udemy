use std::sync::Arc;

use anyhow::Context;
use reanit_api::{
    config::Config,
    infra::{
        clock::SystemClock, health::PgRedisProbe, mailer::SmtpMailer, objects::S3ObjectStore,
        postgres::PgStore, storage::Storage, transcoder::FfmpegTranscoder,
    },
    modules::payments::provider::ConfiguredProviders,
    routes,
    state::AppState,
    worker::VideoWorker,
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

    // reqwest and lettre both use rustls; pick the ring crypto backend once.
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("failed to install TLS crypto provider"))?;

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
    let storage = Arc::new(Storage::new(config.s3.clone()));
    let addr = format!("{}:{}", config.host, config.port);

    let state = AppState {
        config: Arc::new(config),
        users: store.clone(),
        categories: store.clone(),
        instructor_profiles: store.clone(),
        courses: store.clone(),
        curriculum: store.clone(),
        video_assets: store.clone(),
        enrollments: store.clone(),
        saved_courses: store.clone(),
        coupons: store.clone(),
        orders: store.clone(),
        payment_methods: store.clone(),
        payment_providers: Arc::new(ConfiguredProviders::default()),
        refresh_tokens: store.clone(),
        verification_tokens: store,
        mailer,
        clock: Arc::new(SystemClock),
        health: Arc::new(PgRedisProbe { db, redis }),
        storage,
    };
    if state.config.worker_enabled {
        let worker = VideoWorker {
            objects: Arc::new(S3ObjectStore::new(
                state.storage.clone(),
                state.clock.clone(),
            )),
            transcoder: Arc::new(FfmpegTranscoder::default()),
            scratch_dir: state.config.worker_scratch_dir.clone().into(),
            state: state.clone(),
        };
        tokio::spawn(worker.run_forever());
        tracing::info!("video worker started");
    }
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
