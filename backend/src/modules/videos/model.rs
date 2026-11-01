use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoStatus {
    /// Upload URL issued; waiting for the browser to finish uploading.
    Uploading,
    /// Queued for or undergoing transcoding.
    Processing,
    Ready,
    Failed,
}

impl VideoStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            VideoStatus::Uploading => "uploading",
            VideoStatus::Processing => "processing",
            VideoStatus::Ready => "ready",
            VideoStatus::Failed => "failed",
        }
    }
}

impl std::str::FromStr for VideoStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "uploading" => Ok(VideoStatus::Uploading),
            "processing" => Ok(VideoStatus::Processing),
            "ready" => Ok(VideoStatus::Ready),
            "failed" => Ok(VideoStatus::Failed),
            other => Err(format!("unknown video status: {other}")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct VideoAsset {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub lecture_id: Uuid,
    /// Object key of the original upload.
    pub raw_key: String,
    pub status: VideoStatus,
    pub duration_sec: i32,
    pub error: Option<String>,
    /// HLS playlists by file name (`master.m3u8`, `360p.m3u8`, ...). Kept in the
    /// database so playback can rewrite them without fetching from storage.
    pub playlists: BTreeMap<String, String>,
    /// Transcoding attempts so far.
    pub attempts: i32,
    /// When a worker claimed the job (None = not claimed).
    pub claimed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl VideoAsset {
    /// Where the HLS output for this asset lives.
    pub fn hls_prefix(&self) -> String {
        format!("videos/hls/{}", self.id)
    }
}

/// What instructors see about a lecture's video.
#[derive(Debug, Serialize)]
pub struct VideoStatusDto {
    pub asset_id: Uuid,
    pub status: VideoStatus,
    pub duration_sec: i32,
    pub error: Option<String>,
}

impl From<&VideoAsset> for VideoStatusDto {
    fn from(a: &VideoAsset) -> Self {
        Self {
            asset_id: a.id,
            status: a.status,
            duration_sec: a.duration_sec,
            error: a.error.clone(),
        }
    }
}
