//! Checkout: orders, starting payments, polling, provider callbacks.

use std::collections::BTreeMap;

use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    routing::{get, post},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    methods::usable_methods,
    model::{Order, OrderItem, OrderStatus, Payment, PaymentAction, PaymentStatus},
    provider::{PaymentRequest, ProviderKind},
    settle::{refresh, send_receipt},
};
use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::{ApiJson, ApiPath},
    infra::khqr,
    modules::{
        cart::{SavedList, saved_courses},
        coupons::{normalize_codes, price},
        courses::model::{Course, CourseStatus, Currency},
        users::model::User,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders", post(create_order))
        .route("/orders/{id}", get(get_order))
        .route("/orders/{id}/payments", post(start_payment))
        .route("/webhooks/aba-payway", post(aba_payway_callback))
}

/// Mounted under `/me`.
pub fn me_router() -> Router<AppState> {
    Router::new().route("/orders", get(my_orders))
}

// ---------- views ----------

#[derive(Serialize)]
struct QrView {
    string: String,
    svg: String,
}

#[derive(Serialize)]
struct RedirectView {
    url: String,
    fields: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct PaymentView {
    id: Uuid,
    method: String,
    status: PaymentStatus,
    amount_minor: i64,
    currency: Currency,
    reference: String,
    failure_reason: Option<String>,
    qr: Option<QrView>,
    deeplink: Option<String>,
    redirect: Option<RedirectView>,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    paid_at: Option<DateTime<Utc>>,
}

impl From<Payment> for PaymentView {
    fn from(p: Payment) -> Self {
        let (qr, deeplink, redirect) = match p.action {
            PaymentAction::Qr {
                qr_string,
                deeplink,
            } => (
                Some(QrView {
                    svg: khqr::svg(&qr_string),
                    string: qr_string,
                }),
                deeplink,
                None,
            ),
            PaymentAction::Redirect { url, fields } => {
                (None, None, Some(RedirectView { url, fields }))
            }
        };
        Self {
            id: p.id,
            method: p.method,
            status: p.status,
            amount_minor: p.amount_minor,
            currency: p.currency,
            reference: p.reference,
            failure_reason: p.failure_reason,
            qr,
            deeplink,
            redirect,
            expires_at: p.expires_at,
            created_at: p.created_at,
            paid_at: p.paid_at,
        }
    }
}

#[derive(Serialize)]
struct OrderView {
    #[serde(flatten)]
    order: Order,
    /// The most recent payment attempt.
    payment: Option<PaymentView>,
}

#[derive(Serialize)]
struct OrderList {
    data: Vec<Order>,
}

// ---------- create ----------

#[derive(Deserialize)]
struct CreateOrder {
    /// Buy these courses now; otherwise the cart's courses in `currency`.
    course_ids: Option<Vec<Uuid>>,
    currency: Option<Currency>,
    #[serde(default)]
    coupon_codes: Vec<String>,
}

async fn selected_courses(
    state: &AppState,
    user: &User,
    input: &CreateOrder,
) -> AppResult<Vec<Course>> {
    let courses = match &input.course_ids {
        Some(ids) => {
            let mut ids = ids.clone();
            ids.sort();
            ids.dedup();
            let found: Vec<Course> = state
                .courses
                .find_many(&ids)
                .await?
                .into_iter()
                .filter(|c| c.status == CourseStatus::Published)
                .collect();
            if found.len() != ids.len() {
                return Err(AppError::NotFound);
            }
            found
        }
        None => {
            let currency = input.currency.ok_or_else(|| {
                AppError::BadRequest("currency is required to check out the cart".into())
            })?;
            saved_courses(state, SavedList::Cart, user.id)
                .await?
                .into_iter()
                .filter(|c| c.currency == currency)
                .collect()
        }
    };
    if courses.is_empty() {
        return Err(AppError::BadRequest("there is nothing to check out".into()));
    }
    if courses.iter().any(|c| c.currency != courses[0].currency) {
        return Err(AppError::BadRequest(
            "an order can only contain courses priced in one currency".into(),
        ));
    }
    for c in &courses {
        if state.enrollments.is_enrolled(user.id, c.id).await? {
            return Err(AppError::Conflict(format!(
                "you already have \"{}\"",
                c.title
            )));
        }
        if c.instructor_id == user.id {
            return Err(AppError::BadRequest("you can't buy your own course".into()));
        }
        if c.price_minor == 0 {
            return Err(AppError::BadRequest(format!(
                "\"{}\" is free; join it directly",
                c.title
            )));
        }
    }
    Ok(courses)
}

async fn create_order(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(input): ApiJson<CreateOrder>,
) -> AppResult<(StatusCode, Json<OrderView>)> {
    let courses = selected_courses(&state, &user, &input).await?;
    let codes = normalize_codes(input.coupon_codes.iter().map(String::as_str));
    let pricing = price(&state, &courses, &codes).await?;
    let now = state.clock.now();

    let items: Vec<OrderItem> = courses
        .iter()
        .zip(pricing.lines)
        .map(|(c, line)| OrderItem {
            course_id: c.id,
            title: c.title.clone(),
            list_price_minor: line.list_price_minor,
            price_minor: line.price_minor,
            coupon_id: line.coupon_id,
            coupon_code: line.coupon_code,
        })
        .collect();
    let order = Order {
        id: Uuid::new_v4(),
        user_id: user.id,
        status: OrderStatus::Pending,
        currency: courses[0].currency,
        subtotal_minor: items.iter().map(|i| i.list_price_minor).sum(),
        total_minor: items.iter().map(|i| i.price_minor).sum(),
        items,
        created_at: now,
        paid_at: None,
    };
    state.orders.insert_order(&order).await?;

    if order.total_minor == 0 && state.orders.complete_order(order.id, None, now).await? {
        send_receipt(&state, order.id).await;
    }
    let order = load_own_order(&state, &user, order.id).await?;
    Ok((
        StatusCode::CREATED,
        Json(OrderView {
            order,
            payment: None,
        }),
    ))
}

async fn load_own_order(state: &AppState, user: &User, id: Uuid) -> AppResult<Order> {
    state
        .orders
        .find_order(id)
        .await?
        .filter(|o| o.user_id == user.id)
        .ok_or(AppError::NotFound)
}

// ---------- read ----------

async fn get_order(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<OrderView>> {
    let order = load_own_order(&state, &user, id).await?;
    let latest = state
        .orders
        .list_payments(order.id)
        .await?
        .into_iter()
        .next();
    let payment = match latest {
        Some(p) if order.status == OrderStatus::Pending => Some(refresh(&state, p, false).await?),
        other => other,
    };
    // Settling may have just paid the order.
    let order = load_own_order(&state, &user, id).await?;
    Ok(Json(OrderView {
        order,
        payment: payment.map(PaymentView::from),
    }))
}

async fn my_orders(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<OrderList>> {
    Ok(Json(OrderList {
        data: state.orders.list_orders(user.id).await?,
    }))
}

// ---------- pay ----------

#[derive(Deserialize)]
struct StartPayment {
    method: String,
}

/// How long a QR / checkout session stays payable.
const PAYMENT_TTL: Duration = Duration::minutes(10);

async fn start_payment(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(order_id): ApiPath<Uuid>,
    ApiJson(input): ApiJson<StartPayment>,
) -> AppResult<(StatusCode, Json<PaymentView>)> {
    let order = load_own_order(&state, &user, order_id).await?;
    match order.status {
        OrderStatus::Pending => {}
        OrderStatus::Paid => {
            return Err(AppError::Conflict("this order is already paid".into()));
        }
        OrderStatus::Expired => {
            return Err(AppError::Conflict(
                "this order has expired; check out again".into(),
            ));
        }
    }
    let method = usable_methods(&state, order.currency)
        .await?
        .into_iter()
        .find(|m| m.code == input.method)
        .ok_or_else(|| {
            AppError::BadRequest("that payment method isn't available for this order".into())
        })?;
    let provider = state
        .payment_providers
        .get(method.provider)
        .ok_or_else(|| AppError::BadRequest("that payment method isn't available".into()))?;

    let now = state.clock.now();
    let reusable = state
        .orders
        .list_payments(order.id)
        .await?
        .into_iter()
        .find(|p| {
            p.method == method.code && p.status == PaymentStatus::Pending && p.expires_at > now
        });
    if let Some(existing) = reusable {
        return Ok((StatusCode::CREATED, Json(existing.into())));
    }

    let request = PaymentRequest {
        payment_id: Uuid::new_v4(),
        order_id: order.id,
        method: method.code.clone(),
        amount_minor: order.total_minor,
        currency: order.currency,
        description: order
            .items
            .iter()
            .map(|i| i.title.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        payer_name: user.name.clone(),
        payer_email: user.email.clone(),
        created_at: now,
        expires_at: now + PAYMENT_TTL,
    };
    let init = provider.create(&request).await?;
    let payment = Payment {
        id: request.payment_id,
        order_id: order.id,
        method: method.code,
        provider: method.provider,
        amount_minor: request.amount_minor,
        currency: request.currency,
        status: PaymentStatus::Pending,
        reference: init.reference,
        provider_tran_id: None,
        action: init.action,
        failure_reason: None,
        raw_response: Some(init.raw),
        expires_at: request.expires_at,
        last_checked_at: None,
        created_at: now,
        paid_at: None,
    };
    state.orders.insert_payment(&payment).await?;
    Ok((StatusCode::CREATED, Json(payment.into())))
}

// ---------- provider callbacks ----------

/// PayWay's pushback (JSON or form encoded). It is not signed, so it only
/// tells us *which* payment to re-check with PayWay's Check Transaction API.
async fn aba_payway_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<serde_json::Value>> {
    let is_form = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("application/x-www-form-urlencoded"));
    let tran_id: Option<String> = if is_form {
        url_form_value(&body, "tran_id")
    } else {
        serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| match &v["tran_id"] {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
    };

    if let Some(tran_id) = tran_id {
        let payment = state
            .orders
            .find_payment_by_reference(ProviderKind::AbaPayway, &tran_id)
            .await?;
        if let Some(payment) = payment {
            refresh(&state, payment, true).await?;
        } else {
            tracing::warn!(%tran_id, "PayWay callback for an unknown transaction");
        }
    }
    // Always acknowledge so PayWay doesn't retry forever.
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn url_form_value(body: &[u8], key: &str) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    text.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| v.replace('+', " "))
    })
}
