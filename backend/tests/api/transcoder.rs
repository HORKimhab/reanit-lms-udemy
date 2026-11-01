//! Runs the real ffmpeg transcoder on a generated clip. Skipped if ffmpeg is
//! not installed.

use std::path::{Path, PathBuf};

use reanit_api::infra::transcoder::{FfmpegTranscoder, Transcoder};

fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("reanit-transcode-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A 3-second 1280x720 test pattern, optionally with a sine-wave soundtrack.
fn make_clip(dir: &Path, with_audio: bool) -> PathBuf {
    let path = dir.join("input.mp4");
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args([
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=3:size=1280x720:rate=24",
    ]);
    if with_audio {
        cmd.args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=3",
            "-shortest",
        ]);
    }
    cmd.args(["-pix_fmt", "yuv420p"]).arg(&path);
    assert!(
        cmd.status().unwrap().success(),
        "failed to generate test clip"
    );
    path
}

async fn check(with_audio: bool) {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not installed; skipping transcoder test");
        return;
    }
    let dir = scratch_dir();
    let input = make_clip(&dir, with_audio);
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();

    let result = FfmpegTranscoder::default()
        .transcode(&input, &out)
        .await
        .unwrap();

    assert_eq!(result.duration_sec, 3);
    let names: Vec<&str> = result.playlists.keys().map(String::as_str).collect();
    assert_eq!(names, ["360p.m3u8", "720p.m3u8", "master.m3u8"]);
    let master = &result.playlists["master.m3u8"];
    assert!(
        master.contains("360p.m3u8") && master.contains("720p.m3u8"),
        "{master}"
    );
    assert!(
        master.contains("RESOLUTION=640x360") && master.contains("RESOLUTION=1280x720"),
        "{master}"
    );
    assert!(!result.segments.is_empty());
    for segment in &result.segments {
        assert!(segment.ends_with(".ts"), "{segment}");
        assert!(out.join(segment).is_file(), "{segment} missing");
        assert!(
            result.playlists["360p.m3u8"].contains(segment.as_str())
                || result.playlists["720p.m3u8"].contains(segment.as_str())
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn transcodes_a_clip_with_audio_into_two_hls_renditions() {
    check(true).await;
}

#[tokio::test]
async fn transcodes_a_clip_without_audio() {
    check(false).await;
}

#[tokio::test]
async fn a_file_that_is_not_a_video_fails_with_an_error() {
    if !ffmpeg_available() {
        return;
    }
    let dir = scratch_dir();
    let input = dir.join("input.mp4");
    std::fs::write(&input, b"definitely not a video").unwrap();

    let result = FfmpegTranscoder::default().transcode(&input, &dir).await;

    assert!(result.is_err());
    std::fs::remove_dir_all(&dir).ok();
}
