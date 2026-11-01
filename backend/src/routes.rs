use axum::{Router, http::HeaderValue};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{modules, state::AppState};

pub fn build(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(
            state
                .config
                .cors_origin
                .parse::<HeaderValue>()
                .expect("invalid CORS_ORIGIN"),
        )
        .allow_credentials(true)
        .allow_methods(tower_http::cors::AllowMethods::mirror_request())
        .allow_headers(tower_http::cors::AllowHeaders::mirror_request());

    let api = Router::new()
        .merge(modules::health::router())
        .merge(modules::categories::router())
        .nest("/auth", modules::auth::router())
        .nest("/me", modules::me::router())
        .nest("/admin", modules::admin::router());

    Router::new()
        .nest("/api/v1", api)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
