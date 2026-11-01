//! S3-compatible object storage (AWS S3, Cloudflare R2, MinIO).
//! Browsers upload directly with short-lived presigned PUT URLs, so large
//! files never pass through the API.

use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

use crate::config::S3Config;

pub struct PresignRequest<'a> {
    pub method: &'a str,
    pub scheme: &'a str,
    /// Host header value, including a non-default port (e.g. `localhost:9000`).
    pub host: &'a str,
    /// Absolute path, not yet URI-encoded.
    pub path: &'a str,
    pub region: &'a str,
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub now: DateTime<Utc>,
    pub expires_secs: u32,
}

/// AWS Signature V4 query-string presigning (signed header: host; unsigned payload).
pub fn presign_url(req: &PresignRequest) -> String {
    let amz_date = req.now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = req.now.format("%Y%m%d").to_string();
    let scope = format!("{date}/{}/s3/aws4_request", req.region);
    let canonical_uri = encode_path(req.path);

    // Already in sorted order, as SigV4 requires.
    let query = format!(
        "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential={}&X-Amz-Date={amz_date}\
         &X-Amz-Expires={}&X-Amz-SignedHeaders=host",
        uri_encode(&format!("{}/{scope}", req.access_key), true),
        req.expires_secs,
    );
    let canonical_request = format!(
        "{}\n{canonical_uri}\n{query}\nhost:{}\n\nhost\nUNSIGNED-PAYLOAD",
        req.method, req.host
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );

    let key = hmac(
        format!("AWS4{}", req.secret_key).as_bytes(),
        date.as_bytes(),
    );
    let key = hmac(&key, req.region.as_bytes());
    let key = hmac(&key, b"s3");
    let key = hmac(&key, b"aws4_request");
    let signature = hex::encode(hmac(&key, string_to_sign.as_bytes()));

    format!(
        "{}://{}{canonical_uri}?{query}&X-Amz-Signature={signature}",
        req.scheme, req.host
    )
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// RFC 3986 encoding as SigV4 defines it; `/` is kept when encoding paths.
fn uri_encode(input: &str, encode_slash: bool) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b'/' if !encode_slash => out.push('/'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn encode_path(path: &str) -> String {
    uri_encode(path, false)
}

/// Presigns uploads and builds public URLs for one bucket.
pub struct Storage {
    config: S3Config,
}

impl Storage {
    pub fn new(config: S3Config) -> Self {
        Self { config }
    }

    /// URL the browser can `PUT` the object to until it expires.
    pub fn presign_put(&self, key: &str, now: DateTime<Utc>, expires_secs: u32) -> String {
        self.presign("PUT", key, now, expires_secs)
    }

    /// URL to `GET` a private object until it expires.
    pub fn presign_get(&self, key: &str, now: DateTime<Utc>, expires_secs: u32) -> String {
        self.presign("GET", key, now, expires_secs)
    }

    fn presign(&self, method: &str, key: &str, now: DateTime<Utc>, expires_secs: u32) -> String {
        let c = &self.config;
        let (scheme, host) = c
            .endpoint
            .split_once("://")
            .unwrap_or(("https", &c.endpoint));
        let host = host.trim_end_matches('/');
        let (host, path) = if c.path_style {
            (host.to_string(), format!("/{}/{key}", c.bucket))
        } else {
            (format!("{}.{host}", c.bucket), format!("/{key}"))
        };
        presign_url(&PresignRequest {
            method,
            scheme,
            host: &host,
            path: &path,
            region: &c.region,
            access_key: &c.access_key,
            secret_key: &c.secret_key,
            now,
            expires_secs,
        })
    }

    /// Public (CDN / bucket) URL for a publicly readable object.
    pub fn public_url(&self, key: &str) -> String {
        format!("{}/{key}", self.config.public_url.trim_end_matches('/'))
    }
}
