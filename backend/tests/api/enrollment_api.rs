use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::{
    catalog_api::published_course,
    common::TestApp,
    playback_api::{publish_with_preview, ready_video},
    video_api::setup,
};

struct Shop {
    app: TestApp,
    instructor: String,
    admin: String,
    student: String,
}

async fn shop() -> Shop {
    let app = TestApp::new();
    let instructor = app.instructor("teacher@example.com").await;
    let admin = app.admin("admin@example.com").await;
    let student = app.verified_student("student@example.com").await;
    Shop {
        app,
        instructor,
        admin,
        student,
    }
}

impl Shop {
    async fn course(&self, title: &str, fields: Value) -> String {
        published_course(&self.app, &self.instructor, &self.admin, title, fields).await
    }

    async fn enroll(&self, course_id: &str) -> crate::common::TestResponse {
        self.app
            .send(
                Method::POST,
                "/me/courses",
                Some(json!({ "course_id": course_id })),
                &self.student,
            )
            .await
    }

    async fn my_courses(&self) -> Value {
        let res = self.app.get_authed("/me/courses", &self.student).await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        res.body
    }
}

#[tokio::test]
async fn students_enroll_in_free_courses_and_see_them_in_my_learning() {
    let s = shop().await;
    let id = s.course("Free Rust", json!({ "price_minor": 0 })).await;

    let res = s.enroll(&id).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    // Enrolling again is harmless.
    assert_eq!(s.enroll(&id).await.status, StatusCode::OK);

    let mine = s.my_courses().await;
    let list = mine["data"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["course"]["title"], "Free Rust");
    assert_eq!(list[0]["course"]["instructor_name"], "Dara Sok");
    assert!(list[0]["enrolled_at"].is_string());
}

#[tokio::test]
async fn paid_courses_must_be_bought_instead() {
    let s = shop().await;
    let id = s.course("Paid Rust", json!({ "price_minor": 1999 })).await;

    let res = s.enroll(&id).await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert!(
        res.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("purchase"),
        "{:?}",
        res.body
    );
    assert_eq!(s.my_courses().await["data"], json!([]));
}

#[tokio::test]
async fn unpublished_courses_cannot_be_joined() {
    let s = shop().await;
    let draft = s
        .app
        .send(
            Method::POST,
            "/instructor/courses",
            Some(json!({ "title": "Draft" })),
            &s.instructor,
        )
        .await;
    let id = draft.body["id"].as_str().unwrap();

    assert_eq!(s.enroll(id).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn enrolling_requires_login() {
    let s = shop().await;
    let id = s.course("Free Rust", json!({ "price_minor": 0 })).await;

    let res = s
        .app
        .request(
            Method::POST,
            "/me/courses",
            Some(json!({ "course_id": id })),
            None,
            None,
        )
        .await;

    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_course_page_tells_a_student_whether_they_are_enrolled() {
    let s = shop().await;
    let id = s.course("Free Rust", json!({ "price_minor": 0 })).await;
    let slug = s
        .app
        .request(Method::GET, "/courses", None, None, None)
        .await
        .body["data"][0]["slug"]
        .as_str()
        .unwrap()
        .to_string();
    let page = |token: Option<String>| {
        let app = &s.app;
        let path = format!("/courses/{slug}");
        async move {
            app.request(Method::GET, &path, None, token.as_deref(), None)
                .await
                .body
        }
    };

    assert_eq!(page(None).await["viewer"]["enrolled"], false);
    assert_eq!(
        page(Some(s.student.clone())).await["viewer"]["enrolled"],
        false
    );
    s.enroll(&id).await;
    assert_eq!(
        page(Some(s.student.clone())).await["viewer"]["enrolled"],
        true
    );
}

#[tokio::test]
async fn enrolled_students_can_watch_every_lecture() {
    let ctx = setup().await;
    let asset = ready_video(&ctx).await;
    // Free course, video lecture NOT a preview.
    ctx.app
        .send(
            Method::PATCH,
            &ctx.base,
            Some(json!({ "price_minor": 0 })),
            &ctx.token,
        )
        .await;
    publish_with_preview(&ctx, false).await;
    let course_id = ctx.base.rsplit('/').next().unwrap();
    let student = ctx.app.verified_student("student@example.com").await;
    let path = format!("/videos/{asset}/playlist/master.m3u8");

    assert_eq!(
        ctx.app.get_raw(&path, Some(&student)).await.status,
        StatusCode::NOT_FOUND
    );
    let enrolled = ctx
        .app
        .send(
            Method::POST,
            "/me/courses",
            Some(json!({ "course_id": course_id })),
            &student,
        )
        .await;
    assert_eq!(enrolled.status, StatusCode::OK, "{:?}", enrolled.body);
    assert_eq!(
        ctx.app.get_raw(&path, Some(&student)).await.status,
        StatusCode::OK
    );
}
