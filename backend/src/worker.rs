//! Background video transcoding worker. Polls the `video_assets` queue.

use std::{path::PathBuf, sync::Arc, time::Duration as StdDuration};

use chrono::Duration;

use crate::{
    error::AppResult,
    infra::{objects::ObjectStore, transcoder::Transcoder},
    modules::videos::model::{VideoAsset, VideoStatus},
    state::AppState,
};

pub const MAX_ATTEMPTS: i32 = 3;
/// A claim older than this belongs to a worker that died; the job is retried.
const STALE_CLAIM: Duration = Duration::minutes(60);
const IDLE_POLL: StdDuration = StdDuration::from_secs(5);

pub struct VideoWorker {
    pub state: AppState,
    pub objects: Arc<dyn ObjectStore>,
    pub transcoder: Arc<dyn Transcoder>,
    /// Scratch space for downloads and transcoder output.
    pub scratch_dir: PathBuf,
}

impl VideoWorker {
    /// Processes at most one job. Returns whether a job was found.
    pub async fn run_once(&self) -> AppResult<bool> {
        let now = self.state.clock.now();
        let Some(mut asset) = self
            .state
            .video_assets
            .claim_next(now, now - STALE_CLAIM)
            .await?
        else {
            return Ok(false);
        };
        tracing::info!(asset_id = %asset.id, attempt = asset.attempts, "transcoding video");

        let work_dir = self.scratch_dir.join(asset.id.to_string());
        let result = self.process(&asset, &work_dir).await;
        tokio::fs::remove_dir_all(&work_dir).await.ok();

        asset.claimed_at = None;
        asset.updated_at = self.state.clock.now();
        match result {
            Ok(output) => {
                asset.status = VideoStatus::Ready;
                asset.duration_sec = output.duration_sec;
                asset.playlists = output.playlists;
                asset.error = None;
                self.state.video_assets.save(&asset).await?;
                self.update_lecture_duration(&asset).await?;
                tracing::info!(asset_id = %asset.id, "video ready");
            }
            Err(err) => {
                tracing::warn!(asset_id = %asset.id, error = ?err, "transcoding failed");
                asset.error = Some(format!("{err:#}"));
                if asset.attempts >= MAX_ATTEMPTS {
                    asset.status = VideoStatus::Failed;
                }
                self.state.video_assets.save(&asset).await?;
            }
        }
        Ok(true)
    }

    async fn process(
        &self,
        asset: &VideoAsset,
        work_dir: &std::path::Path,
    ) -> anyhow::Result<crate::infra::transcoder::TranscodeOutput> {
        let out_dir = work_dir.join("hls");
        tokio::fs::create_dir_all(&out_dir).await?;
        let input = work_dir.join("input");
        self.objects.download(&asset.raw_key, &input).await?;

        let output = self.transcoder.transcode(&input, &out_dir).await?;
        for segment in &output.segments {
            let key = format!("{}/{segment}", asset.hls_prefix());
            self.objects
                .upload(&out_dir.join(segment), &key, "video/mp2t")
                .await?;
        }
        Ok(output)
    }

    /// Copies the duration onto the lecture, if it still uses this video.
    async fn update_lecture_duration(&self, asset: &VideoAsset) -> AppResult<()> {
        if let Some(mut lecture) = self.state.curriculum.find_lecture(asset.lecture_id).await?
            && lecture.video_asset_id == Some(asset.id)
        {
            lecture.duration_sec = asset.duration_sec;
            self.state.curriculum.save_lecture(&lecture).await?;
        }
        Ok(())
    }

    /// Polls forever, sleeping when the queue is empty.
    pub async fn run_forever(self) {
        loop {
            match self.run_once().await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(err) => tracing::error!(error = ?err, "video worker error"),
            }
            tokio::time::sleep(IDLE_POLL).await;
        }
    }
}
