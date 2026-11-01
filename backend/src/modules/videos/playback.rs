//! HLS playback. Playlists are served by the API (after an access check);
//! segment URIs are rewritten to short-lived presigned storage URLs, so the
//! bucket itself stays private.

use axum::{
    Router,
    extract::State,
    http::header,
    response::{IntoResponse, Response},
    routing::get,
};
use uuid::Uuid;

use super::model::{VideoAsset, VideoStatus};
use crate::{
    auth::extractor::CurrentUser,
    error::{AppError, AppResult},
    extract::ApiPath,
    modules::users::model::{Role, User},
    state::AppState,
};

/// Signed segment URLs outlive the video by this much (pauses, seeking).
const SEGMENT_URL_GRACE_SECS: u32 = 3 * 3600;

pub fn router() -> Router<AppState> {
    Router::new().route("/videos/{asset_id}/playlist/{name}", get(playlist))
}

/// Who may watch a video. For now: the course's instructor and admins.
/// (Enrolled students and free previews arrive with enrollments.)
async fn can_watch(state: &AppState, user: &User, asset: &VideoAsset) -> AppResult<bool> {
    if user.role == Role::Admin {
        return Ok(true);
    }
    let Some(lecture) = state.curriculum.find_lecture(asset.lecture_id).await? else {
        return Ok(false);
    };
    let Some(course) = state.courses.find(lecture.course_id).await? else {
        return Ok(false);
    };
    Ok(course.instructor_id == user.id)
}

async fn playlist(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath((asset_id, name)): ApiPath<(Uuid, String)>,
) -> AppResult<Response> {
    let asset = state
        .video_assets
        .find(asset_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !can_watch(&state, &user, &asset).await? {
        return Err(AppError::NotFound);
    }
    if asset.status != VideoStatus::Ready {
        return Err(AppError::Conflict("video is not ready yet".into()));
    }
    let playlist = asset.playlists.get(&name).ok_or(AppError::NotFound)?;

    let body = if name == "master.m3u8" {
        // Variant URIs are relative, so they resolve back to this endpoint.
        playlist.clone()
    } else {
        let ttl = u32::try_from(asset.duration_sec).unwrap_or(0) + SEGMENT_URL_GRACE_SECS;
        sign_segments(&state, &asset, playlist, ttl)
    };

    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.apple.mpegurl"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        body,
    )
        .into_response())
}

/// Replaces every URI line (non-empty, not a `#` tag) with a presigned URL.
fn sign_segments(state: &AppState, asset: &VideoAsset, playlist: &str, ttl_secs: u32) -> String {
    let now = state.clock.now();
    let mut out = String::with_capacity(playlist.len() * 4);
    for line in playlist.lines() {
        if line.is_empty() || line.starts_with('#') {
            out.push_str(line);
        } else {
            let key = format!("{}/{line}", asset.hls_prefix());
            out.push_str(&state.storage.presign_get(&key, now, ttl_secs));
        }
        out.push('\n');
    }
    out
}
