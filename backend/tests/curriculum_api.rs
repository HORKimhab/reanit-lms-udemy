mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::{Value, json};

struct Ctx {
    app: TestApp,
    token: String,
    base: String,
}

async fn setup() -> Ctx {
    let app = TestApp::new();
    let token = app.instructor("dara@example.com").await;
    let res = app
        .send(
            Method::POST,
            "/instructor/courses",
            Some(json!({ "title": "Rust" })),
            &token,
        )
        .await;
    let base = format!("/instructor/courses/{}", res.body["id"].as_str().unwrap());
    Ctx { app, token, base }
}

impl Ctx {
    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> common::TestResponse {
        self.app
            .send(method, &format!("{}{path}", self.base), body, &self.token)
            .await
    }

    async fn add_section(&self, title: &str) -> String {
        let res = self
            .call(Method::POST, "/sections", Some(json!({ "title": title })))
            .await;
        assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
        res.body["id"].as_str().unwrap().to_string()
    }

    async fn add_lecture(&self, section: &str, title: &str, kind: &str) -> String {
        let res = self
            .call(
                Method::POST,
                &format!("/sections/{section}/lectures"),
                Some(json!({ "title": title, "kind": kind })),
            )
            .await;
        assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
        res.body["id"].as_str().unwrap().to_string()
    }

    async fn curriculum(&self) -> Value {
        let res = self.call(Method::GET, "/curriculum", None).await;
        assert_eq!(res.status, StatusCode::OK);
        res.body
    }
}

fn titles(curriculum: &Value) -> Vec<(String, Vec<String>)> {
    curriculum
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let lectures = s["lectures"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l["title"].as_str().unwrap().to_string())
                .collect();
            (s["title"].as_str().unwrap().to_string(), lectures)
        })
        .collect()
}

fn owned(pairs: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    pairs
        .iter()
        .map(|(s, ls)| (s.to_string(), ls.iter().map(|l| l.to_string()).collect()))
        .collect()
}

#[tokio::test]
async fn sections_and_lectures_are_returned_in_the_order_they_were_added() {
    let ctx = setup().await;
    let intro = ctx.add_section("Introduction").await;
    let basics = ctx.add_section("Basics").await;
    ctx.add_lecture(&intro, "Welcome", "video").await;
    ctx.add_lecture(&intro, "Setup", "article").await;
    ctx.add_lecture(&basics, "Variables", "video").await;

    let curriculum = ctx.curriculum().await;

    assert_eq!(
        titles(&curriculum),
        owned(&[
            ("Introduction", &["Welcome", "Setup"]),
            ("Basics", &["Variables"])
        ])
    );
    let welcome = &curriculum[0]["lectures"][0];
    assert_eq!(welcome["kind"], "video");
    assert_eq!(welcome["is_preview"], false);
}

#[tokio::test]
async fn sections_and_lectures_can_be_edited() {
    let ctx = setup().await;
    let section = ctx.add_section("Intro").await;
    let article = ctx.add_lecture(&section, "Read me", "article").await;
    let video = ctx.add_lecture(&section, "Watch me", "video").await;

    let renamed = ctx
        .call(
            Method::PATCH,
            &format!("/sections/{section}"),
            Some(json!({ "title": "Introduction" })),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    assert_eq!(renamed.body["title"], "Introduction");

    let edited = ctx
        .call(
            Method::PATCH,
            &format!("/lectures/{article}"),
            Some(json!({ "title": "Read this", "is_preview": true, "article_body": "# Hello" })),
        )
        .await;
    assert_eq!(edited.status, StatusCode::OK);
    assert_eq!(edited.body["title"], "Read this");
    assert_eq!(edited.body["is_preview"], true);
    assert_eq!(edited.body["article_body"], "# Hello");

    let body_on_video = ctx
        .call(
            Method::PATCH,
            &format!("/lectures/{video}"),
            Some(json!({ "article_body": "nope" })),
        )
        .await;
    assert_eq!(body_on_video.status, StatusCode::BAD_REQUEST);

    let blank = ctx
        .call(
            Method::PATCH,
            &format!("/lectures/{video}"),
            Some(json!({ "title": " " })),
        )
        .await;
    assert_eq!(blank.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn deleting_a_section_removes_its_lectures() {
    let ctx = setup().await;
    let keep = ctx.add_section("Keep").await;
    let drop = ctx.add_section("Drop").await;
    ctx.add_lecture(&keep, "Stays", "video").await;
    let gone = ctx.add_lecture(&drop, "Goes", "video").await;

    assert_eq!(
        ctx.call(Method::DELETE, &format!("/sections/{drop}"), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );

    assert_eq!(
        titles(&ctx.curriculum().await),
        owned(&[("Keep", &["Stays"])])
    );
    let patch_gone = ctx
        .call(
            Method::PATCH,
            &format!("/lectures/{gone}"),
            Some(json!({ "title": "x" })),
        )
        .await;
    assert_eq!(patch_gone.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_lecture_removes_only_that_lecture() {
    let ctx = setup().await;
    let section = ctx.add_section("Intro").await;
    let a = ctx.add_lecture(&section, "A", "video").await;
    ctx.add_lecture(&section, "B", "video").await;

    assert_eq!(
        ctx.call(Method::DELETE, &format!("/lectures/{a}"), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );

    assert_eq!(titles(&ctx.curriculum().await), owned(&[("Intro", &["B"])]));
}

#[tokio::test]
async fn the_whole_curriculum_can_be_reordered_including_moving_lectures_between_sections() {
    let ctx = setup().await;
    let s1 = ctx.add_section("One").await;
    let s2 = ctx.add_section("Two").await;
    let a = ctx.add_lecture(&s1, "A", "video").await;
    let b = ctx.add_lecture(&s1, "B", "video").await;
    let c = ctx.add_lecture(&s2, "C", "video").await;

    let res = ctx
        .call(
            Method::PUT,
            "/curriculum/order",
            Some(json!({ "sections": [
                { "id": s2, "lecture_ids": [c, a] },
                { "id": s1, "lecture_ids": [b] },
            ]})),
        )
        .await;

    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(
        titles(&res.body),
        owned(&[("Two", &["C", "A"]), ("One", &["B"])])
    );
    assert_eq!(titles(&ctx.curriculum().await), titles(&res.body));
}

#[tokio::test]
async fn reordering_must_list_every_section_and_lecture_exactly_once() {
    let ctx = setup().await;
    let s1 = ctx.add_section("One").await;
    let a = ctx.add_lecture(&s1, "A", "video").await;
    let b = ctx.add_lecture(&s1, "B", "video").await;
    let foreign = uuid::Uuid::new_v4().to_string();

    for order in [
        json!({ "sections": [{ "id": s1, "lecture_ids": [a] }] }),
        json!({ "sections": [{ "id": s1, "lecture_ids": [a, b, a] }] }),
        json!({ "sections": [{ "id": s1, "lecture_ids": [a, b, foreign] }] }),
        json!({ "sections": [] }),
    ] {
        let res = ctx
            .call(Method::PUT, "/curriculum/order", Some(order.clone()))
            .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{order}");
    }
}

#[tokio::test]
async fn other_instructors_cannot_touch_the_curriculum() {
    let ctx = setup().await;
    let section = ctx.add_section("Intro").await;
    let other = ctx.app.instructor("other@example.com").await;

    let path = format!("{}/sections", ctx.base);
    let res = ctx
        .app
        .send(
            Method::POST,
            &path,
            Some(json!({ "title": "Hijack" })),
            &other,
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    let get = ctx
        .app
        .send(
            Method::GET,
            &format!("{}/curriculum", ctx.base),
            None,
            &other,
        )
        .await;
    assert_eq!(get.status, StatusCode::NOT_FOUND);

    // Their own course can't be used to reach this section either.
    let theirs = ctx
        .app
        .send(
            Method::POST,
            "/instructor/courses",
            Some(json!({ "title": "Mine" })),
            &other,
        )
        .await;
    let theirs = theirs.body["id"].as_str().unwrap();
    let res = ctx
        .app
        .send(
            Method::PATCH,
            &format!("/instructor/courses/{theirs}/sections/{section}"),
            Some(json!({ "title": "Hijack" })),
            &other,
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}
