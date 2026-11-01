use std::{collections::BTreeMap, path::Path, sync::Arc};

use async_trait::async_trait;
use axum::http::{Method, StatusCode, header};
use reanit_api::infra::transcoder::{TranscodeOutput, Transcoder};

use crate::video_api::{VideoCtx, setup};

pub struct CannedTranscoder;

#[async_trait]
impl Transcoder for CannedTranscoder {
    async fn transcode(&self, _input: &Path, out_dir: &Path) -> anyhow::Result<TranscodeOutput> {
        std::fs::write(out_dir.join("720p_000.ts"), b"seg")?;
        let mut playlists = BTreeMap::new();
        playlists.insert(
            "master.m3u8".into(),
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=3000000,RESOLUTION=1280x720\n720p.m3u8\n".into(),
        );
        playlists.insert(
            "720p.m3u8".into(),
            "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\n720p_000.ts\n#EXT-X-ENDLIST\n".into(),
        );
        Ok(TranscodeOutput {
            duration_sec: 6,
            playlists,
            segments: vec!["720p_000.ts".into()],
        })
    }
}

/// A lecture with a ready video; returns the asset id.
pub async fn ready_video(ctx: &VideoCtx) -> String {
    let asset = ctx.upload_and_attach().await;
    let asset = asset.as_str().unwrap().to_string();
    ctx.app
        .objects
        .put(&format!("videos/raw/{asset}.mp4"), b"raw".to_vec());
    ctx.app
        .video_worker(Arc::new(CannedTranscoder))
        .run_once()
        .await
        .unwrap();
    asset
}

#[tokio::test]
async fn the_master_playlist_is_served_as_is() {
    let ctx = setup().await;
    let asset = ready_video(&ctx).await;

    let res = ctx
        .app
        .get_raw(
            &format!("/videos/{asset}/playlist/master.m3u8"),
            Some(&ctx.token),
        )
        .await;

    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(
        res.headers[header::CONTENT_TYPE],
        "application/vnd.apple.mpegurl"
    );
    assert!(res.text.contains("\n720p.m3u8\n"), "{}", res.text);
}

#[tokio::test]
async fn segment_uris_in_variant_playlists_become_presigned_storage_urls() {
    let ctx = setup().await;
    let asset = ready_video(&ctx).await;

    let res = ctx
        .app
        .get_raw(
            &format!("/videos/{asset}/playlist/720p.m3u8"),
            Some(&ctx.token),
        )
        .await;

    assert_eq!(res.status, StatusCode::OK);
    let lines: Vec<&str> = res.text.lines().collect();
    assert_eq!(lines[0], "#EXTM3U");
    assert_eq!(lines[2], "#EXTINF:6.0,");
    assert!(
        lines[3].starts_with(&format!(
            "http://localhost:9000/reanit-media/videos/hls/{asset}/720p_000.ts?"
        )),
        "{}",
        lines[3]
    );
    assert!(lines[3].contains("X-Amz-Signature="));
    assert_eq!(lines[4], "#EXT-X-ENDLIST");
}

#[tokio::test]
async fn unknown_playlists_and_unready_videos_are_not_served() {
    let ctx = setup().await;
    let processing = ctx.upload_and_attach().await;
    let processing = processing.as_str().unwrap();

    let not_ready = ctx
        .app
        .get_raw(
            &format!("/videos/{processing}/playlist/master.m3u8"),
            Some(&ctx.token),
        )
        .await;
    assert_eq!(not_ready.status, StatusCode::CONFLICT);

    let unknown_asset = uuid::Uuid::new_v4();
    let res = ctx
        .app
        .get_raw(
            &format!("/videos/{unknown_asset}/playlist/master.m3u8"),
            Some(&ctx.token),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_people_allowed_to_watch_get_the_playlist() {
    let ctx = setup().await;
    let asset = ready_video(&ctx).await;
    let path = format!("/videos/{asset}/playlist/master.m3u8");

    assert_eq!(
        ctx.app.get_raw(&path, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    let other = ctx.app.instructor("other@example.com").await;
    assert_eq!(
        ctx.app.get_raw(&path, Some(&other)).await.status,
        StatusCode::NOT_FOUND
    );
    let student = ctx.app.verified_student("student@example.com").await;
    assert_eq!(
        ctx.app.get_raw(&path, Some(&student)).await.status,
        StatusCode::NOT_FOUND
    );
    let admin = ctx.app.admin("admin@example.com").await;
    assert_eq!(
        ctx.app.get_raw(&path, Some(&admin)).await.status,
        StatusCode::OK
    );

    let missing = ctx
        .app
        .get_raw(
            &format!("/videos/{asset}/playlist/1080p.m3u8"),
            Some(&ctx.token),
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let _ = Method::GET;
}

/// Makes the video lecture a free preview and publishes the course.
pub async fn publish_with_preview(ctx: &VideoCtx, preview: bool) {
    let app = &ctx.app;
    let lecture = &ctx.video_lecture;
    app.send(
        Method::PATCH,
        &format!("{}/lectures/{lecture}", ctx.base),
        Some(serde_json::json!({ "is_preview": preview })),
        &ctx.token,
    )
    .await;
    // The setup's article lecture needs content before the course can be submitted.
    app.send(
        Method::PATCH,
        &format!("{}/lectures/{}", ctx.base, ctx.article_lecture),
        Some(serde_json::json!({ "article_body": "Hello" })),
        &ctx.token,
    )
    .await;
    crate::submit_api::complete_details(app, &ctx.token, &ctx.base).await;
    let submitted = app
        .send(
            Method::POST,
            &format!("{}/submit", ctx.base),
            None,
            &ctx.token,
        )
        .await;
    assert_eq!(submitted.status, StatusCode::OK, "{:?}", submitted.body);
    let admin = app.admin("admin@example.com").await;
    let id = ctx.base.rsplit('/').next().unwrap();
    app.send(
        Method::POST,
        &format!("/admin/courses/{id}/approve"),
        None,
        &admin,
    )
    .await;
}

#[tokio::test]
async fn anyone_can_watch_a_free_preview_of_a_published_course() {
    let ctx = setup().await;
    let asset = ready_video(&ctx).await;
    publish_with_preview(&ctx, true).await;

    let anonymous = ctx
        .app
        .get_raw(&format!("/videos/{asset}/playlist/720p.m3u8"), None)
        .await;
    assert_eq!(anonymous.status, StatusCode::OK);
    assert!(anonymous.text.contains("X-Amz-Signature="));

    let student = ctx.app.verified_student("student@example.com").await;
    let logged_in = ctx
        .app
        .get_raw(
            &format!("/videos/{asset}/playlist/master.m3u8"),
            Some(&student),
        )
        .await;
    assert_eq!(logged_in.status, StatusCode::OK);
}

#[tokio::test]
async fn non_preview_lectures_still_require_access() {
    let ctx = setup().await;
    let asset = ready_video(&ctx).await;
    publish_with_preview(&ctx, false).await;
    let path = format!("/videos/{asset}/playlist/master.m3u8");

    assert_eq!(
        ctx.app.get_raw(&path, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    let student = ctx.app.verified_student("student@example.com").await;
    assert_eq!(
        ctx.app.get_raw(&path, Some(&student)).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn previews_of_unpublished_courses_are_private() {
    let ctx = setup().await;
    let asset = ready_video(&ctx).await;
    ctx.app
        .send(
            Method::PATCH,
            &format!("{}/lectures/{}", ctx.base, ctx.video_lecture),
            Some(serde_json::json!({ "is_preview": true })),
            &ctx.token,
        )
        .await;

    let res = ctx
        .app
        .get_raw(&format!("/videos/{asset}/playlist/master.m3u8"), None)
        .await;

    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_invalid_token_is_rejected_even_for_previews() {
    let ctx = setup().await;
    let asset = ready_video(&ctx).await;
    publish_with_preview(&ctx, true).await;

    let res = ctx
        .app
        .get_raw(
            &format!("/videos/{asset}/playlist/master.m3u8"),
            Some("garbage"),
        )
        .await;

    assert_eq!(
        res.status,
        StatusCode::UNAUTHORIZED,
        "lets the client refresh its session"
    );
}
