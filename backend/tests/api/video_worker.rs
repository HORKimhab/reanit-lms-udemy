use std::{collections::BTreeMap, path::Path, sync::Arc};

use async_trait::async_trait;
use axum::http::StatusCode;
use reanit_api::infra::transcoder::{FfmpegTranscoder, TranscodeOutput, Transcoder};

use crate::video_api::setup;

/// Writes two fake segments and returns canned playlists; can be told to fail.
struct FakeTranscoder {
    fail: bool,
}

#[async_trait]
impl Transcoder for FakeTranscoder {
    async fn transcode(&self, input: &Path, out_dir: &Path) -> anyhow::Result<TranscodeOutput> {
        if self.fail {
            anyhow::bail!("corrupt input");
        }
        assert_eq!(
            std::fs::read(input)?,
            b"raw video bytes",
            "worker downloads the raw upload"
        );
        std::fs::write(out_dir.join("720p_000.ts"), b"seg0")?;
        std::fs::write(out_dir.join("720p_001.ts"), b"seg1")?;
        let mut playlists = BTreeMap::new();
        playlists.insert("master.m3u8".into(), "#EXTM3U\n720p.m3u8\n".into());
        playlists.insert(
            "720p.m3u8".into(),
            "#EXTM3U\n720p_000.ts\n720p_001.ts\n".into(),
        );
        Ok(TranscodeOutput {
            duration_sec: 125,
            playlists,
            segments: vec!["720p_000.ts".into(), "720p_001.ts".into()],
        })
    }
}

#[tokio::test]
async fn the_worker_transcodes_queued_videos_and_marks_them_ready() {
    let ctx = setup().await;
    let asset = ctx.upload_and_attach().await;
    let asset_id = asset.as_str().unwrap();
    ctx.app.objects.put(
        &format!("videos/raw/{asset_id}.mp4"),
        b"raw video bytes".to_vec(),
    );
    let worker = ctx
        .app
        .video_worker(Arc::new(FakeTranscoder { fail: false }));

    assert!(worker.run_once().await.unwrap(), "a job was processed");

    let status = ctx.status(&ctx.video_lecture).await;
    assert_eq!(status.body["status"], "ready");
    assert_eq!(status.body["duration_sec"], 125);
    assert_eq!(
        ctx.app
            .objects
            .get(&format!("videos/hls/{asset_id}/720p_001.ts"))
            .as_deref(),
        Some(&b"seg1"[..])
    );
    let curriculum = ctx
        .app
        .send(
            axum::http::Method::GET,
            &format!("{}/curriculum", ctx.base),
            None,
            &ctx.token,
        )
        .await;
    assert_eq!(curriculum.body[0]["lectures"][0]["duration_sec"], 125);

    assert!(!worker.run_once().await.unwrap(), "queue is empty");
}

#[tokio::test]
async fn failed_jobs_are_retried_then_marked_failed_with_the_error() {
    let ctx = setup().await;
    let asset = ctx.upload_and_attach().await;
    ctx.app.objects.put(
        &format!("videos/raw/{}.mp4", asset.as_str().unwrap()),
        b"raw video bytes".to_vec(),
    );
    let worker = ctx
        .app
        .video_worker(Arc::new(FakeTranscoder { fail: true }));

    for attempt in 1..=2 {
        assert!(worker.run_once().await.unwrap());
        assert_eq!(
            ctx.status(&ctx.video_lecture).await.body["status"],
            "processing",
            "attempt {attempt}"
        );
    }
    assert!(worker.run_once().await.unwrap());

    let status = ctx.status(&ctx.video_lecture).await;
    assert_eq!(status.body["status"], "failed");
    assert!(
        status.body["error"]
            .as_str()
            .unwrap()
            .contains("corrupt input")
    );
    assert!(
        !worker.run_once().await.unwrap(),
        "failed jobs leave the queue"
    );
}

#[tokio::test]
async fn a_video_that_was_never_uploaded_fails() {
    let ctx = setup().await;
    ctx.upload_and_attach().await;
    let worker = ctx
        .app
        .video_worker(Arc::new(FakeTranscoder { fail: false }));

    for _ in 0..3 {
        worker.run_once().await.unwrap();
    }

    let status = ctx.status(&ctx.video_lecture).await;
    assert_eq!(status.status, StatusCode::OK);
    assert_eq!(status.body["status"], "failed");
}

#[tokio::test]
async fn end_to_end_with_real_ffmpeg() {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        return;
    }
    let ctx = setup().await;
    let asset = ctx.upload_and_attach().await;
    let asset_id = asset.as_str().unwrap();
    let clip = std::env::temp_dir().join(format!("reanit-e2e-{asset_id}.mp4"));
    let ok = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=640x360:rate=24",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&clip)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    ctx.app.objects.put(
        &format!("videos/raw/{asset_id}.mp4"),
        std::fs::read(&clip).unwrap(),
    );
    std::fs::remove_file(&clip).ok();

    let worker = ctx.app.video_worker(Arc::new(FfmpegTranscoder::default()));
    worker.run_once().await.unwrap();

    let status = ctx.status(&ctx.video_lecture).await;
    assert_eq!(status.body["status"], "ready", "{:?}", status.body);
    assert_eq!(status.body["duration_sec"], 2);
    assert!(
        ctx.app
            .objects
            .get(&format!("videos/hls/{asset_id}/360p_000.ts"))
            .is_some()
    );
}
