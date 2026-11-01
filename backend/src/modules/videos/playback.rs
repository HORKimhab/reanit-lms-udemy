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
    auth::extractor::OptionalUser,
    error::{AppError, AppResult},
    extract::ApiPath,
    modules::{
        courses::model::CourseStatus,
        users::model::{Role, User},
    },
    state::AppState,
};

/// Signed segment URLs outlive the video by this much (pauses, seeking).
const SEGMENT_URL_GRACE_SECS: u32 = 3 * 3600;

pub fn router() -> Router<AppState> {
    Router::new().route("/videos/{asset_id}/playlist/{name}", get(playlist))
}

enum Access {
    Allowed,
    /// Not logged in, and the video isn't public.
    LoginRequired,
    Denied,
}

/// Who may watch a video: anyone for the free-preview lecture of a published
/// course; otherwise enrolled students, the course's instructor and admins.
async fn access(state: &AppState, user: Option<&User>, asset: &VideoAsset) -> AppResult<Access> {
    let Some(lecture) = state.curriculum.find_lecture(asset.lecture_id).await? else {
        return Ok(Access::Denied);
    };
    let Some(course) = state.courses.find(lecture.course_id).await? else {
        return Ok(Access::Denied);
    };

    let is_current_video = lecture.video_asset_id == Some(asset.id);
    if is_current_video && lecture.is_preview && course.status == CourseStatus::Published {
        return Ok(Access::Allowed);
    }
    Ok(match user {
        None => Access::LoginRequired,
        Some(u) if u.role == Role::Admin || u.id == course.instructor_id => Access::Allowed,
        Some(u) if state.enrollments.is_enrolled(u.id, course.id).await? => Access::Allowed,
        Some(_) => Access::Denied,
    })
}

async fn playlist(
    State(state): State<AppState>,
    OptionalUser(user): OptionalUser,
    ApiPath((asset_id, name)): ApiPath<(Uuid, String)>,
) -> AppResult<Response> {
    let asset = state
        .video_assets
        .find(asset_id)
        .await?
        .ok_or(AppError::NotFound)?;
    match access(&state, user.as_ref(), &asset).await? {
        Access::Allowed => {}
        Access::LoginRequired => return Err(AppError::Unauthorized),
        Access::Denied => return Err(AppError::NotFound),
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
