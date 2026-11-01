//! Video transcoding to adaptive HLS with ffmpeg.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use async_trait::async_trait;
use tokio::process::Command;

#[derive(Debug)]
pub struct TranscodeOutput {
    pub duration_sec: i32,
    /// Playlist file name -> contents (`master.m3u8`, `360p.m3u8`, `720p.m3u8`).
    pub playlists: BTreeMap<String, String>,
    /// Segment file names, relative to the output directory.
    pub segments: Vec<String>,
}

#[async_trait]
pub trait Transcoder: Send + Sync {
    /// Transcodes `input` into HLS files inside `out_dir`.
    async fn transcode(&self, input: &Path, out_dir: &Path) -> anyhow::Result<TranscodeOutput>;
}

/// Shells out to `ffmpeg` / `ffprobe` (must be on PATH or configured).
pub struct FfmpegTranscoder {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl Default for FfmpegTranscoder {
    fn default() -> Self {
        Self {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
        }
    }
}

/// (name, max height, video bitrate, max rate, buffer size)
const RENDITIONS: [(&str, u32, &str, &str, &str); 2] = [
    ("360p", 360, "800k", "856k", "1200k"),
    ("720p", 720, "2800k", "2996k", "4200k"),
];
const SEGMENT_SECONDS: &str = "6";

async fn run(cmd: &mut Command) -> anyhow::Result<String> {
    let output = cmd
        .output()
        .await
        .context("failed to start ffmpeg/ffprobe")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr.lines().rev().take(5).collect::<Vec<_>>().join(" | ");
        bail!(
            "{} failed: {tail}",
            cmd.as_std().get_program().to_string_lossy()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

impl FfmpegTranscoder {
    async fn probe_duration(&self, input: &Path) -> anyhow::Result<f64> {
        let out = run(Command::new(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(input))
        .await?;
        out.parse::<f64>()
            .with_context(|| format!("unexpected duration {out:?}"))
    }

    async fn has_audio(&self, input: &Path) -> anyhow::Result<bool> {
        let out = run(Command::new(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "a",
                "-show_entries",
                "stream=index",
                "-of",
                "csv=p=0",
            ])
            .arg(input))
        .await?;
        Ok(!out.is_empty())
    }
}

#[async_trait]
impl Transcoder for FfmpegTranscoder {
    async fn transcode(&self, input: &Path, out_dir: &Path) -> anyhow::Result<TranscodeOutput> {
        let duration = self.probe_duration(input).await?;
        let audio = self.has_audio(input).await?;

        // Split the video once per rendition; never upscale; keep heights even.
        let splits: String = (0..RENDITIONS.len()).map(|i| format!("[s{i}]")).collect();
        let mut filter = format!("[0:v]split={}{splits}", RENDITIONS.len());
        for (i, (name, height, ..)) in RENDITIONS.iter().enumerate() {
            filter.push_str(&format!(
                ";[s{i}]scale=-2:'trunc(min({height},ih)/2)*2'[v{name}]"
            ));
        }

        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args(["-v", "error", "-y", "-i"])
            .arg(input)
            .args(["-filter_complex", &filter]);
        for (i, (name, _, bitrate, maxrate, bufsize)) in RENDITIONS.iter().enumerate() {
            cmd.args(["-map", &format!("[v{name}]")])
                .args([&format!("-c:v:{i}"), "libx264"])
                .args([format!("-b:v:{i}").as_str(), bitrate])
                .args([format!("-maxrate:v:{i}").as_str(), maxrate])
                .args([format!("-bufsize:v:{i}").as_str(), bufsize]);
        }
        if audio {
            for _ in RENDITIONS {
                cmd.args(["-map", "0:a:0"]);
            }
            cmd.args(["-c:a", "aac", "-b:a", "128k", "-ac", "2"]);
        }
        let stream_map = RENDITIONS
            .iter()
            .enumerate()
            .map(|(i, (name, ..))| {
                if audio {
                    format!("v:{i},a:{i},name:{name}")
                } else {
                    format!("v:{i},name:{name}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        cmd.args(["-preset", "veryfast", "-pix_fmt", "yuv420p"])
            // Fixed 2-second GOPs so every segment starts on a keyframe.
            .args([
                "-force_key_frames",
                "expr:gte(t,n_forced*2)",
                "-sc_threshold",
                "0",
            ])
            .args([
                "-f",
                "hls",
                "-hls_time",
                SEGMENT_SECONDS,
                "-hls_playlist_type",
                "vod",
            ])
            .args(["-hls_flags", "independent_segments"])
            .arg("-hls_segment_filename")
            .arg(out_dir.join("%v_%03d.ts"))
            .args([
                "-master_pl_name",
                "master.m3u8",
                "-var_stream_map",
                &stream_map,
            ])
            .arg(out_dir.join("%v.m3u8"));
        run(&mut cmd).await?;

        let mut playlists = BTreeMap::new();
        let mut segments = Vec::new();
        let mut entries = tokio::fs::read_dir(out_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".m3u8") {
                playlists.insert(name, tokio::fs::read_to_string(entry.path()).await?);
            } else if name.ends_with(".ts") {
                segments.push(name);
            }
        }
        segments.sort();
        if !playlists.contains_key("master.m3u8") {
            bail!("ffmpeg produced no master playlist");
        }

        Ok(TranscodeOutput {
            duration_sec: duration.round() as i32,
            playlists,
            segments,
        })
    }
}
