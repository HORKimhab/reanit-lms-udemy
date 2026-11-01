//! Payment options: public listing for checkout, admin management.

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{get, patch},
};
use serde::{Deserialize, Serialize};

use super::{model::PaymentMethod, repo::MethodPatch};
use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::{ApiJson, ApiPath},
    modules::{courses::model::Currency, users::model::Role},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/payments/methods", get(available))
}

/// Mounted under `/admin`.
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/payment-methods", get(list_all))
        .route("/payment-methods/{code}", patch(update))
}

#[derive(Serialize)]
struct MethodList {
    data: Vec<PaymentMethod>,
}

/// Enabled methods whose provider is configured and that accept `currency`.
pub async fn usable_methods(state: &AppState, currency: Currency) -> AppResult<Vec<PaymentMethod>> {
    Ok(state
        .payment_methods
        .list_methods()
        .await?
        .into_iter()
        .filter(|m| {
            m.enabled
                && m.currencies.contains(&currency)
                && state.payment_providers.get(m.provider).is_some()
        })
        .collect())
}

#[derive(Deserialize)]
struct AvailableQuery {
    currency: Currency,
}

async fn available(
    State(state): State<AppState>,
    Query(q): Query<AvailableQuery>,
) -> AppResult<Json<MethodList>> {
    Ok(Json(MethodList {
        data: usable_methods(&state, q.currency).await?,
    }))
}

async fn list_all(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<Json<MethodList>> {
    current.require_role(Role::Admin)?;
    Ok(Json(MethodList {
        data: state.payment_methods.list_methods().await?,
    }))
}

async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(code): ApiPath<String>,
    ApiJson(patch): ApiJson<MethodPatch>,
) -> AppResult<Json<PaymentMethod>> {
    current.require_role(Role::Admin)?;
    if patch.label.as_deref().is_some_and(|l| l.trim().is_empty()) {
        return Err(AppError::BadRequest("label cannot be empty".into()));
    }
    if patch.currencies.as_ref().is_some_and(Vec::is_empty) {
        return Err(AppError::BadRequest("pick at least one currency".into()));
    }
    let updated = state
        .payment_methods
        .update_method(&code, &patch)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(updated))
}
