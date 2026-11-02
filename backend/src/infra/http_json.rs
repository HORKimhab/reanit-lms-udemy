//! Small JSON-over-HTTP helper for provider APIs (reqwest without its `json` feature).

use std::time::Duration;

use crate::error::{AppError, AppResult};

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .expect("HTTP client")
}

/// POSTs `body` as JSON; returns the HTTP status and parsed JSON response
/// (`Null` when the body is empty or not JSON).
pub async fn post_json(
    http: &reqwest::Client,
    url: &str,
    bearer: Option<&str>,
    body: &serde_json::Value,
) -> AppResult<(u16, serde_json::Value)> {
    let mut req = http
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT, "application/json")
        .body(serde_json::to_vec(body).map_err(anyhow::Error::from)?);
    if let Some(token) = bearer {
        req = req.bearer_auth(token);
    }
    let res = req
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("{url}: {e}")))?;
    let status = res.status().as_u16();
    let bytes = res
        .bytes()
        .await
        .map_err(|e| AppError::Upstream(format!("{url}: {e}")))?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    ))
}

/// Provider amounts arrive as JSON numbers in major units (`19.99`, `40000`).
pub fn minor_units(
    value: &serde_json::Value,
    currency: crate::modules::courses::model::Currency,
) -> Option<i64> {
    use crate::modules::courses::model::Currency;
    let major = match value {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.parse().ok()?,
        _ => return None,
    };
    Some(match currency {
        Currency::Usd => (major * 100.0).round() as i64,
        Currency::Khr => major.round() as i64,
    })
}
