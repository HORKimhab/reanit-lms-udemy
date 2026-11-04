//! Background payment checker: asks providers about pending payments so they
//! settle (or expire) even when nobody is polling the order.

use std::time::Duration as StdDuration;

use chrono::Duration;

use super::settle::refresh;
use crate::{error::AppResult, state::AppState};

/// Payments checked per pass.
const BATCH: i64 = 50;
/// Unpaid orders older than this (with no payment in progress) expire.
pub const ORDER_TTL: Duration = Duration::hours(24);
const POLL_EVERY: StdDuration = StdDuration::from_secs(5);

pub struct PaymentWorker {
    pub state: AppState,
}

impl PaymentWorker {
    /// One pass: checks pending payments, then expires stale orders.
    /// Returns how many payments were checked.
    pub async fn run_once(&self) -> AppResult<usize> {
        let pending = self.state.orders.list_pending_payments(BATCH).await?;
        let count = pending.len();
        for payment in pending {
            let id = payment.id;
            if let Err(err) = refresh(&self.state, payment, true).await {
                tracing::warn!(payment = %id, error = ?err, "payment check failed");
            }
        }
        let cutoff = self.state.clock.now() - ORDER_TTL;
        let expired = self.state.orders.expire_stale_orders(cutoff).await?;
        if expired > 0 {
            tracing::info!(expired, "expired stale orders");
        }
        Ok(count)
    }

    /// Polls forever.
    pub async fn run_forever(self) {
        loop {
            if let Err(err) = self.run_once().await {
                tracing::error!(error = ?err, "payment worker error");
            }
            tokio::time::sleep(POLL_EVERY).await;
        }
    }
}
