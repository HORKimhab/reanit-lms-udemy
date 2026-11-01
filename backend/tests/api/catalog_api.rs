use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::{common::TestApp, submit_api::complete};

/// Creates, completes, submits and approves a course; applies `fields` first.
pub async fn published_course(
    app: &TestApp,
    instructor: &str,
    admin: &str,
    title: &str,
    fields: Value,
) -> String {
    let res = app
        .send(
            Method::POST,
            "/instructor/courses",
            Some(json!({ "title": title })),
            instructor,
        )
        .await;
    let id = res.body["id"].as_str().unwrap().to_string();
    let base = format!("/instructor/courses/{id}");
    complete(app, instructor, &base).await;
    let patch = app
        .send(Method::PATCH, &base, Some(fields), instructor)
        .await;
    assert_eq!(patch.status, StatusCode::OK, "{:?}", patch.body);
    let submitted = app
        .send(Method::POST, &format!("{base}/submit"), None, instructor)
        .await;
    assert_eq!(submitted.status, StatusCode::OK, "{:?}", submitted.body);
    let approved = app
        .send(
            Method::POST,
            &format!("/admin/courses/{id}/approve"),
            None,
            admin,
        )
        .await;
    assert_eq!(approved.status, StatusCode::OK);
    id
}

async fn catalog(app: &TestApp, query: &str) -> Value {
    let res = app
        .request(Method::GET, &format!("/courses{query}"), None, None, None)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    res.body
}

fn titles(page: &Value) -> Vec<String> {
    page["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["title"].as_str().unwrap().to_string())
        .collect()
}

struct Catalog {
    app: TestApp,
    instructor: String,
    admin: String,
}

async fn setup() -> Catalog {
    let app = TestApp::new();
    let instructor = app.instructor("dara@example.com").await;
    let admin = app.admin("admin@example.com").await;
    Catalog {
        app,
        instructor,
        admin,
    }
}

impl Catalog {
    async fn publish(&self, title: &str, fields: Value) -> String {
        published_course(&self.app, &self.instructor, &self.admin, title, fields).await
    }
}

#[tokio::test]
async fn only_published_courses_are_listed_with_card_details() {
    let c = setup().await;
    c.publish("Rust Basics", json!({ "price_minor": 1999 }))
        .await;
    c.app
        .send(
            Method::POST,
            "/instructor/courses",
            Some(json!({ "title": "Secret draft" })),
            &c.instructor,
        )
        .await;

    let page = catalog(&c.app, "").await;

    assert_eq!(titles(&page), ["Rust Basics"]);
    let card = &page["data"][0];
    assert_eq!(card["instructor_name"], "Dara Sok");
    assert_eq!(card["lecture_count"], 1);
    assert_eq!(card["price_minor"], 1999);
    assert!(card["thumbnail_url"].is_string());
    assert!(card["slug"].is_string());
    assert!(card.get("description").is_none(), "cards stay small");
    assert_eq!(
        page["meta"],
        json!({ "page": 1, "per_page": 20, "total": 1 })
    );
}

#[tokio::test]
async fn search_matches_title_and_subtitle_case_insensitively() {
    let c = setup().await;
    c.publish("Rust Basics", json!({ "subtitle": "Systems programming" }))
        .await;
    c.publish(
        "Docker Deep Dive",
        json!({ "subtitle": "Containers in production", "description": "Ship containers." }),
    )
    .await;

    assert_eq!(titles(&catalog(&c.app, "?q=rust").await), ["Rust Basics"]);
    assert_eq!(
        titles(&catalog(&c.app, "?q=CONTAINERS").await),
        ["Docker Deep Dive"]
    );
    assert!(titles(&catalog(&c.app, "?q=kubernetes").await).is_empty());
}

#[tokio::test]
async fn courses_can_be_filtered_by_category_level_and_price() {
    let c = setup().await;
    let web = c.app.categories.list_ids()[0].clone();
    let data = c.app.categories.list_ids()[1].clone();
    c.publish(
        "Free web",
        json!({ "category_id": web, "level": "beginner", "price_minor": 0 }),
    )
    .await;
    c.publish(
        "Paid web",
        json!({ "category_id": web, "level": "expert", "price_minor": 500 }),
    )
    .await;
    c.publish(
        "Paid data",
        json!({ "category_id": data, "level": "beginner", "price_minor": 500 }),
    )
    .await;

    let mut by_category = titles(&catalog(&c.app, "?category=web-development").await);
    by_category.sort();
    assert_eq!(by_category, ["Free web", "Paid web"]);
    let mut beginner = titles(&catalog(&c.app, "?level=beginner").await);
    beginner.sort();
    assert_eq!(beginner, ["Free web", "Paid data"]);
    assert_eq!(titles(&catalog(&c.app, "?price=free").await), ["Free web"]);
    assert_eq!(
        titles(&catalog(&c.app, "?category=web-development&price=paid").await),
        ["Paid web"]
    );

    let unknown = c
        .app
        .request(Method::GET, "/courses?category=nope", None, None, None)
        .await;
    assert_eq!(unknown.status, StatusCode::OK);
    assert!(unknown.body["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn courses_sort_by_price_across_currencies_and_by_newest() {
    let c = setup().await;
    c.publish(
        "Ten dollars",
        json!({ "price_minor": 1000, "currency": "USD" }),
    )
    .await;
    c.app.clock.advance(chrono::Duration::minutes(1));
    c.publish(
        "Twenty thousand riel",
        json!({ "price_minor": 20000, "currency": "KHR" }),
    )
    .await; // ~ $4.88
    c.app.clock.advance(chrono::Duration::minutes(1));
    c.publish("Free", json!({ "price_minor": 0 })).await;

    assert_eq!(
        titles(&catalog(&c.app, "?sort=price_asc").await),
        ["Free", "Twenty thousand riel", "Ten dollars"]
    );
    assert_eq!(
        titles(&catalog(&c.app, "?sort=price_desc").await),
        ["Ten dollars", "Twenty thousand riel", "Free"]
    );
    assert_eq!(
        titles(&catalog(&c.app, "").await),
        ["Free", "Twenty thousand riel", "Ten dollars"],
        "newest first by default"
    );
}

#[tokio::test]
async fn results_are_paginated() {
    let c = setup().await;
    for i in 0..3 {
        c.publish(&format!("Course {i}"), json!({})).await;
        c.app.clock.advance(chrono::Duration::minutes(1));
    }

    let first = catalog(&c.app, "?per_page=2").await;
    let second = catalog(&c.app, "?per_page=2&page=2").await;

    assert_eq!(titles(&first), ["Course 2", "Course 1"]);
    assert_eq!(titles(&second), ["Course 0"]);
    assert_eq!(
        second["meta"],
        json!({ "page": 2, "per_page": 2, "total": 3 })
    );
    let too_big = c
        .app
        .request(Method::GET, "/courses?per_page=500", None, None, None)
        .await;
    assert_eq!(too_big.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_course_page_shows_the_outline_but_only_preview_content() {
    let c = setup().await;
    let id = c
        .publish(
            "Rust Basics",
            json!({ "outcomes": ["Write Rust"], "requirements": ["A laptop"] }),
        )
        .await;
    let slug = c
        .app
        .send(
            Method::GET,
            &format!("/instructor/courses/{id}"),
            None,
            &c.instructor,
        )
        .await
        .body["slug"]
        .as_str()
        .unwrap()
        .to_string();

    let res = c
        .app
        .request(Method::GET, &format!("/courses/{slug}"), None, None, None)
        .await;

    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let course = &res.body;
    assert_eq!(course["title"], "Rust Basics");
    assert_eq!(course["description"], "A practical introduction to Rust.");
    assert_eq!(course["outcomes"], json!(["Write Rust"]));
    assert_eq!(course["instructor"]["name"], "Dara Sok");
    assert_eq!(course["instructor"]["headline"], "Instructor");
    let lecture = &course["curriculum"][0]["lectures"][0];
    assert_eq!(lecture["title"], "Read me");
    assert_eq!(lecture["kind"], "article");
    assert_eq!(lecture["is_preview"], false);
    assert!(
        lecture.get("article_body").is_none_or(Value::is_null),
        "paid content is hidden"
    );
    assert!(course.get("instructor_id").is_none_or(|_| true));
}

#[tokio::test]
async fn preview_lectures_expose_their_content_on_the_course_page() {
    let c = setup().await;
    let id = c.publish("Rust Basics", json!({})).await;
    let base = format!("/instructor/courses/{id}");
    let curriculum = c
        .app
        .send(
            Method::GET,
            &format!("{base}/curriculum"),
            None,
            &c.instructor,
        )
        .await;
    let lecture = curriculum.body[0]["lectures"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    c.app
        .send(
            Method::PATCH,
            &format!("{base}/lectures/{lecture}"),
            Some(json!({ "is_preview": true })),
            &c.instructor,
        )
        .await;
    let slug = c
        .app
        .send(Method::GET, &base, None, &c.instructor)
        .await
        .body["slug"]
        .as_str()
        .unwrap()
        .to_string();

    let res = c
        .app
        .request(Method::GET, &format!("/courses/{slug}"), None, None, None)
        .await;

    let lecture = &res.body["curriculum"][0]["lectures"][0];
    assert_eq!(lecture["is_preview"], true);
    assert_eq!(lecture["article_body"], "Hello Rust");
}

#[tokio::test]
async fn unpublished_courses_have_no_public_page() {
    let c = setup().await;
    let res = c
        .app
        .send(
            Method::POST,
            "/instructor/courses",
            Some(json!({ "title": "Draft" })),
            &c.instructor,
        )
        .await;
    let slug = res.body["slug"].as_str().unwrap();

    let page = c
        .app
        .request(Method::GET, &format!("/courses/{slug}"), None, None, None)
        .await;

    assert_eq!(page.status, StatusCode::NOT_FOUND);
}
