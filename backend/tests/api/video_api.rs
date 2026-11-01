use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::common::{TestApp, TestResponse};

pub struct VideoCtx {
    pub app: TestApp,
    pub token: String,
    pub base: String,
    pub video_lecture: String,
    pub article_lecture: String,
}

pub async fn setup() -> VideoCtx {
    let app = TestApp::new();
    let token = app.instructor("dara@example.com").await;
    let course = app
        .send(
            Method::POST,
            "/instructor/courses",
            Some(json!({ "title": "Rust" })),
            &token,
        )
        .await;
    let base = format!(
        "/instructor/courses/{}",
        course.body["id"].as_str().unwrap()
    );
    let section = app
        .send(
            Method::POST,
            &format!("{base}/sections"),
            Some(json!({ "title": "S" })),
            &token,
        )
        .await;
    let section = section.body["id"].as_str().unwrap().to_string();
    let lecture = |kind: &'static str| {
        let (app, token, base, section) = (&app, &token, &base, &section);
        async move {
            let res = app
                .send(
                    Method::POST,
                    &format!("{base}/sections/{section}/lectures"),
                    Some(json!({ "title": kind, "kind": kind })),
                    token,
                )
                .await;
            res.body["id"].as_str().unwrap().to_string()
        }
    };
    let video_lecture = lecture("video").await;
    let article_lecture = lecture("article").await;
    VideoCtx {
        app,
        token,
        base,
        video_lecture,
        article_lecture,
    }
}

impl VideoCtx {
    pub async fn upload_url(&self, lecture: &str, content_type: &str) -> TestResponse {
        self.app
            .send(
                Method::POST,
                &format!("{}/lectures/{lecture}/video/upload-url", self.base),
                Some(json!({ "content_type": content_type })),
                &self.token,
            )
            .await
    }

    pub async fn attach(&self, lecture: &str, asset_id: &Value) -> TestResponse {
        self.app
            .send(
                Method::POST,
                &format!("{}/lectures/{lecture}/video", self.base),
                Some(json!({ "asset_id": asset_id })),
                &self.token,
            )
            .await
    }

    pub async fn status(&self, lecture: &str) -> TestResponse {
        self.app
            .send(
                Method::GET,
                &format!("{}/lectures/{lecture}/video", self.base),
                None,
                &self.token,
            )
            .await
    }

    /// Uploads (pretend) and attaches a video; returns the asset id.
    pub async fn upload_and_attach(&self) -> Value {
        let upload = self.upload_url(&self.video_lecture, "video/mp4").await;
        let asset = upload.body["asset_id"].clone();
        assert_eq!(
            self.attach(&self.video_lecture, &asset).await.status,
            StatusCode::OK
        );
        asset
    }
}

#[tokio::test]
async fn instructors_get_an_upload_url_for_a_video_lecture() {
    let ctx = setup().await;

    let res = ctx.upload_url(&ctx.video_lecture, "video/mp4").await;

    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let asset = res.body["asset_id"].as_str().unwrap();
    let url = res.body["upload_url"].as_str().unwrap();
    assert!(
        url.starts_with(&format!(
            "http://localhost:9000/reanit-media/videos/raw/{asset}.mp4?"
        )),
        "{url}"
    );
    assert_eq!(res.body["expires_in_secs"], 3600);
}

#[tokio::test]
async fn only_video_files_for_video_lectures_are_accepted() {
    let ctx = setup().await;

    assert_eq!(
        ctx.upload_url(&ctx.article_lecture, "video/mp4")
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        ctx.upload_url(&ctx.video_lecture, "image/png").await.status,
        StatusCode::BAD_REQUEST
    );
    for ok in [
        "video/mp4",
        "video/quicktime",
        "video/webm",
        "video/x-matroska",
    ] {
        assert_eq!(
            ctx.upload_url(&ctx.video_lecture, ok).await.status,
            StatusCode::OK,
            "{ok}"
        );
    }
}

#[tokio::test]
async fn attaching_an_uploaded_video_queues_it_for_processing() {
    let ctx = setup().await;
    assert_eq!(
        ctx.status(&ctx.video_lecture).await.status,
        StatusCode::NOT_FOUND
    );

    let asset = ctx.upload_and_attach().await;

    let status = ctx.status(&ctx.video_lecture).await;
    assert_eq!(status.status, StatusCode::OK);
    assert_eq!(status.body["asset_id"], asset);
    assert_eq!(status.body["status"], "processing");
    let curriculum = ctx
        .app
        .send(
            Method::GET,
            &format!("{}/curriculum", ctx.base),
            None,
            &ctx.token,
        )
        .await;
    assert_eq!(curriculum.body[0]["lectures"][0]["video_asset_id"], asset);
}

#[tokio::test]
async fn an_asset_can_only_be_attached_once_and_only_to_its_own_lecture() {
    let ctx = setup().await;
    let section = ctx
        .app
        .send(
            Method::GET,
            &format!("{}/curriculum", ctx.base),
            None,
            &ctx.token,
        )
        .await;
    let section = section.body[0]["id"].as_str().unwrap().to_string();
    let other = ctx
        .app
        .send(
            Method::POST,
            &format!("{}/sections/{section}/lectures", ctx.base),
            Some(json!({ "title": "Other", "kind": "video" })),
            &ctx.token,
        )
        .await;
    let other = other.body["id"].as_str().unwrap().to_string();
    let asset = ctx.upload_url(&ctx.video_lecture, "video/mp4").await.body["asset_id"].clone();

    assert_eq!(
        ctx.attach(&other, &asset).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        ctx.attach(&ctx.video_lecture, &asset).await.status,
        StatusCode::OK
    );
    assert_eq!(
        ctx.attach(&ctx.video_lecture, &asset).await.status,
        StatusCode::CONFLICT
    );
    let unknown = json!(uuid::Uuid::new_v4().to_string());
    assert_eq!(
        ctx.attach(&ctx.video_lecture, &unknown).await.status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn other_instructors_cannot_upload_to_the_lecture() {
    let ctx = setup().await;
    let other = ctx.app.instructor("other@example.com").await;

    let res = ctx
        .app
        .send(
            Method::POST,
            &format!(
                "{}/lectures/{}/video/upload-url",
                ctx.base, ctx.video_lecture
            ),
            Some(json!({ "content_type": "video/mp4" })),
            &other,
        )
        .await;

    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_course_cannot_be_submitted_while_a_video_is_still_processing() {
    let ctx = setup().await;
    ctx.upload_and_attach().await;

    let res = ctx
        .app
        .send(
            Method::POST,
            &format!("{}/submit", ctx.base),
            None,
            &ctx.token,
        )
        .await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    let message = res.body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("lecture \"video\" is still processing"),
        "{message}"
    );
}
