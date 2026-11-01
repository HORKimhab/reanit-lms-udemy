//! Turning provider answers into order state. The provider is the only source
//! of truth: callbacks and client polls merely trigger a check.

use chrono::Duration;

use super::{
    model::{Order, Payment, PaymentStatus},
    provider::ProviderStatus,
    repo::Settlement,
};
use crate::{
    error::{AppError, AppResult},
    infra::mailer::Email,
    modules::courses::model::Currency,
    state::AppState,
};

/// Polls closer together than this reuse the last answer.
pub const MIN_CHECK_INTERVAL: Duration = Duration::seconds(3);

/// Re-checks a pending payment with its provider and records the outcome.
/// `force` skips the polling throttle (provider callbacks, background worker).
pub async fn refresh(state: &AppState, payment: Payment, force: bool) -> AppResult<Payment> {
    if payment.status != PaymentStatus::Pending {
        return Ok(payment);
    }
    let now = state.clock.now();
    let recently_checked = payment
        .last_checked_at
        .is_some_and(|t| now - t < MIN_CHECK_INTERVAL);
    if recently_checked && !force {
        return Ok(payment);
    }
    let Some(provider) = state.payment_providers.get(payment.provider) else {
        return Ok(payment);
    };
    state.orders.touch_payment(payment.id, now).await?;

    let status = match provider.check(&payment).await {
        Ok(status) => status,
        Err(err) => {
            // Provider hiccups must not break polling; we'll ask again.
            tracing::warn!(payment = %payment.id, error = ?err, "payment status check failed");
            ProviderStatus::Pending
        }
    };

    match status {
        ProviderStatus::Paid {
            provider_tran_id,
            amount_minor,
            currency,
            raw,
        } => {
            if amount_minor != payment.amount_minor || currency != payment.currency {
                tracing::error!(
                    payment = %payment.id,
                    expected = payment.amount_minor,
                    got = amount_minor,
                    "provider reported a different amount; not enrolling"
                );
                state
                    .orders
                    .close_payment(
                        payment.id,
                        PaymentStatus::Failed,
                        Some("paid amount does not match the order"),
                        Some(&raw),
                    )
                    .await?;
            } else {
                let settlement = Settlement {
                    payment_id: payment.id,
                    provider_tran_id,
                    raw,
                };
                let completed = state
                    .orders
                    .complete_order(payment.order_id, Some(&settlement), now)
                    .await?;
                if completed {
                    send_receipt(state, payment.order_id).await;
                } else {
                    tracing::warn!(
                        order = %payment.order_id,
                        payment = %payment.id,
                        "payment received for an order that was already paid; refund needed"
                    );
                }
            }
        }
        ProviderStatus::Failed { reason, raw } => {
            state
                .orders
                .close_payment(payment.id, PaymentStatus::Failed, Some(&reason), Some(&raw))
                .await?;
        }
        ProviderStatus::Pending if now >= payment.expires_at => {
            state
                .orders
                .close_payment(payment.id, PaymentStatus::Expired, None, None)
                .await?;
        }
        ProviderStatus::Pending => {}
    }

    state
        .orders
        .list_payments(payment.order_id)
        .await?
        .into_iter()
        .find(|p| p.id == payment.id)
        .ok_or(AppError::NotFound)
}

/// `$19.99` / `40,000៛`.
pub fn format_money(amount_minor: i64, currency: Currency) -> String {
    match currency {
        Currency::Usd => format!("${}.{:02}", amount_minor / 100, amount_minor % 100),
        Currency::Khr => {
            let digits = amount_minor.to_string();
            let mut grouped = String::new();
            for (i, c) in digits.chars().enumerate() {
                if i > 0 && (digits.len() - i).is_multiple_of(3) {
                    grouped.push(',');
                }
                grouped.push(c);
            }
            format!("{grouped}៛")
        }
    }
}

/// Best effort: a failed email never undoes a purchase.
pub async fn send_receipt(state: &AppState, order_id: uuid::Uuid) {
    let result: AppResult<()> = async {
        let order: Order = state
            .orders
            .find_order(order_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let user = state
            .users
            .find_by_id(order.user_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let lines: Vec<String> = order
            .items
            .iter()
            .map(|i| format!("- {}: {}", i.title, format_money(i.price_minor, order.currency)))
            .collect();
        let body = format!(
            "Hi {},\n\nThanks for your purchase! Order {}\n\n{}\n\nTotal: {}\n\nStart learning: {}/my-learning\n",
            user.name,
            order.id,
            lines.join("\n"),
            format_money(order.total_minor, order.currency),
            state.config.frontend_url,
        );
        state
            .mailer
            .send(Email {
                to: user.email,
                subject: "Your ReanIT receipt".into(),
                body,
            })
            .await
    }
    .await;
    if let Err(err) = result {
        tracing::warn!(order = %order_id, error = ?err, "failed to send receipt");
    }
}
