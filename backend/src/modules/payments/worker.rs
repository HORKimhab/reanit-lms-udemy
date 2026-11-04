//! Background payment checker: asks providers about pending payments so they
//! settle (or expire) even when nobody is polling the order.

use std::time::Duration as StdDuration;

use super::settle::refresh;
use crate::{error::AppResult, state::AppState};

/// Payments checked per pass.
const BATCH: i64 = 50;
const POLL_EVERY: StdDuration = StdDuration::from_secs(5);

pub struct PaymentWorker {
    pub state: AppState,
}

impl PaymentWorker {
    /// One pass over pending payments. Returns how many were checked.
    pub async fn run_once(&self) -> AppResult<usize> {
        let pending = self.state.orders.list_pending_payments(BATCH).await?;
        let count = pending.len();
        for payment in pending {
            let id = payment.id;
            if let Err(err) = refresh(&self.state, payment, true).await {
                tracing::warn!(payment = %id, error = ?err, "payment check failed");
            }
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
