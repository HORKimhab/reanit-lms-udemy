//! Server-side object transfer (the worker downloading raw uploads and
//! uploading HLS output).

use std::{path::Path, sync::Arc};

use anyhow::{Context, bail};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use super::{clock::Clock, storage::Storage};

#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Streams the object to `dest` (raw videos can be gigabytes).
    async fn download(&self, key: &str, dest: &Path) -> anyhow::Result<()>;
    async fn upload(&self, src: &Path, key: &str, content_type: &str) -> anyhow::Result<()>;
}

/// S3-compatible storage over presigned URLs.
pub struct S3ObjectStore {
    storage: Arc<Storage>,
    clock: Arc<dyn Clock>,
    http: reqwest::Client,
}

const TRANSFER_URL_TTL_SECS: u32 = 3600;

impl S3ObjectStore {
    pub fn new(storage: Arc<Storage>, clock: Arc<dyn Clock>) -> Self {
        Self {
            storage,
            clock,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ObjectStore for S3ObjectStore {
    async fn download(&self, key: &str, dest: &Path) -> anyhow::Result<()> {
        let url = self
            .storage
            .presign_get(key, self.clock.now(), TRANSFER_URL_TTL_SECS);
        let mut res = self
            .http
            .get(url)
            .send()
            .await
            .context("download request failed")?;
        if !res.status().is_success() {
            bail!("download of {key} failed with HTTP {}", res.status());
        }
        let mut file = tokio::fs::File::create(dest).await?;
        while let Some(chunk) = res.chunk().await? {
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        Ok(())
    }

    async fn upload(&self, src: &Path, key: &str, content_type: &str) -> anyhow::Result<()> {
        let url = self
            .storage
            .presign_put(key, self.clock.now(), TRANSFER_URL_TTL_SECS);
        // HLS segments are a few MB, so buffering them is fine.
        let body = tokio::fs::read(src).await?;
        let res = self
            .http
            .put(url)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body)
            .send()
            .await
            .context("upload request failed")?;
        if !res.status().is_success() {
            bail!("upload of {key} failed with HTTP {}", res.status());
        }
        Ok(())
    }
}
