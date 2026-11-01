use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::model::{VideoAsset, VideoStatus, VideoStatusDto};
use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::{ApiJson, ApiPath},
    modules::curriculum::{
        handlers::{editable_course, lecture_of, managed_course},
        model::LectureKind,
    },
    state::AppState,
};

/// Large uploads can take a while on slow connections.
const UPLOAD_URL_TTL_SECS: u32 = 3600;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/courses/{course_id}/lectures/{lecture_id}/video/upload-url",
            post(upload_url),
        )
        .route(
            "/courses/{course_id}/lectures/{lecture_id}/video",
            get(status).post(attach),
        )
}

fn video_extension(content_type: &str) -> Option<&'static str> {
    match content_type {
        "video/mp4" => Some("mp4"),
        "video/quicktime" => Some("mov"),
        "video/webm" => Some("webm"),
        "video/x-matroska" => Some("mkv"),
        _ => None,
    }
}

#[derive(Deserialize)]
struct UploadUrlRequest {
    content_type: String,
}

#[derive(Serialize)]
struct UploadUrlResponse {
    asset_id: Uuid,
    upload_url: String,
    expires_in_secs: u32,
}

/// Step 1: create an asset and a presigned URL the browser uploads the raw file to.
async fn upload_url(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((course_id, lecture_id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<UploadUrlRequest>,
) -> AppResult<Json<UploadUrlResponse>> {
    let course = editable_course(&state, &current, course_id).await?;
    let lecture = lecture_of(&state, &course, lecture_id).await?;
    if lecture.kind != LectureKind::Video {
        return Err(AppError::BadRequest(
            "only video lectures can have a video".into(),
        ));
    }
    let ext = video_extension(&req.content_type)
        .ok_or_else(|| AppError::BadRequest("video must be MP4, MOV, WebM or MKV".into()))?;

    let now = state.clock.now();
    let id = Uuid::new_v4();
    let asset = VideoAsset {
        id,
        owner_id: current.0.id,
        lecture_id: lecture.id,
        raw_key: format!("videos/raw/{id}.{ext}"),
        status: VideoStatus::Uploading,
        duration_sec: 0,
        error: None,
        playlists: BTreeMap::new(),
        attempts: 0,
        claimed_at: None,
        created_at: now,
        updated_at: now,
    };
    state.video_assets.insert(&asset).await?;

    let upload_url = state
        .storage
        .presign_put(&asset.raw_key, now, UPLOAD_URL_TTL_SECS);
    Ok(Json(UploadUrlResponse {
        asset_id: id,
        upload_url,
        expires_in_secs: UPLOAD_URL_TTL_SECS,
    }))
}

#[derive(Deserialize)]
struct AttachRequest {
    asset_id: Uuid,
}

/// Step 2 (after the browser finished uploading): attach the video to the
/// lecture and queue it for transcoding.
async fn attach(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((course_id, lecture_id)): ApiPath<(Uuid, Uuid)>,
    ApiJson(req): ApiJson<AttachRequest>,
) -> AppResult<Json<VideoStatusDto>> {
    let course = editable_course(&state, &current, course_id).await?;
    let mut lecture = lecture_of(&state, &course, lecture_id).await?;
    let mut asset = match state.video_assets.find(req.asset_id).await? {
        Some(a) if a.lecture_id == lecture.id => a,
        _ => return Err(AppError::BadRequest("unknown video upload".into())),
    };
    if asset.status != VideoStatus::Uploading {
        return Err(AppError::Conflict(
            "this video was already submitted".into(),
        ));
    }

    asset.status = VideoStatus::Processing;
    asset.updated_at = state.clock.now();
    state.video_assets.save(&asset).await?;

    lecture.video_asset_id = Some(asset.id);
    lecture.duration_sec = 0;
    state.curriculum.save_lecture(&lecture).await?;
    Ok(Json((&asset).into()))
}

async fn status(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((course_id, lecture_id)): ApiPath<(Uuid, Uuid)>,
) -> AppResult<Json<VideoStatusDto>> {
    let course = managed_course(&state, &current, course_id).await?;
    let lecture = lecture_of(&state, &course, lecture_id).await?;
    let asset_id = lecture.video_asset_id.ok_or(AppError::NotFound)?;
    let asset = state
        .video_assets
        .find(asset_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json((&asset).into()))
}
