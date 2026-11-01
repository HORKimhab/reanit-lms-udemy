//! Round-trip tests for the Postgres adapters. They need a real database and
//! are skipped (pass trivially) unless `DATABASE_URL` is set, e.g. in CI or
//! locally after `docker compose up -d`.

use chrono::{Duration, DurationRound, Utc};
use reanit_api::{
    auth::store::{RefreshTokenRepo, VerificationTokenRepo},
    error::AppError,
    infra::postgres::PgStore,
    modules::users::{
        model::{NewUser, Role},
        repo::UserRepo,
    },
    modules::{
        categories::CategoryRepo,
        courses::{
            model::{Course, CourseStatus, Currency, Level},
            repo::{CatalogFilter, CatalogSort, CourseRepo, PriceFilter},
        },
        curriculum::{
            model::{Lecture, LectureKind, Section},
            repo::{CurriculumRepo, SectionOrder},
        },
        instructors::{InstructorProfile, InstructorProfileRepo},
        videos::{
            model::{VideoAsset, VideoStatus},
            repo::VideoAssetRepo,
        },
    },
};
use uuid::Uuid;

async fn store() -> Option<PgStore> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL not set; skipping Postgres tests");
        return None;
    };
    let pool = sqlx::PgPool::connect(&url)
        .await
        .expect("connect to Postgres");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrate");
    Some(PgStore::new(pool))
}

fn unique_email() -> String {
    format!("user-{}@example.com", Uuid::new_v4())
}

fn new_user(email: &str) -> NewUser {
    NewUser {
        name: "Dara".into(),
        email: email.into(),
        password_hash: "$argon2id$fake".into(),
    }
}

/// Postgres stores microseconds; truncate so round-tripped values compare equal.
fn now() -> chrono::DateTime<Utc> {
    Utc::now()
        .duration_trunc(Duration::microseconds(1))
        .unwrap()
}

#[tokio::test]
async fn users_round_trip_with_case_insensitive_unique_email() {
    let Some(store) = store().await else { return };
    let email = unique_email();

    let created = store.create(new_user(&email), now()).await.unwrap();
    assert_eq!(created.email, email);
    assert_eq!(created.role, Role::Student);
    assert!(created.email_verified_at.is_none());

    let found = store
        .find_by_email(&email.to_uppercase())
        .await
        .unwrap()
        .expect("found");
    assert_eq!(found.id, created.id);
    assert_eq!(
        store.find_by_id(created.id).await.unwrap().unwrap().email,
        email
    );

    let dup = store.create(new_user(&email.to_uppercase()), now()).await;
    assert!(matches!(dup, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn users_can_be_verified_once_and_change_role() {
    let Some(store) = store().await else { return };
    let user = store
        .create(new_user(&unique_email()), now())
        .await
        .unwrap();

    let first = now();
    let verified = store
        .mark_email_verified(user.id, first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(verified.email_verified_at, Some(first));
    let again = store
        .mark_email_verified(user.id, first + Duration::hours(1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        again.email_verified_at,
        Some(first),
        "first verification time is kept"
    );

    let admin = store.set_role(user.id, Role::Admin).await.unwrap().unwrap();
    assert_eq!(admin.role, Role::Admin);
    assert!(
        store
            .set_role(Uuid::new_v4(), Role::Admin)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn refresh_tokens_revoke_once_and_revoke_all() {
    let Some(store) = store().await else { return };
    let user = store
        .create(new_user(&unique_email()), now())
        .await
        .unwrap();
    let (a, b) = (
        format!("hash-{}", Uuid::new_v4()),
        format!("hash-{}", Uuid::new_v4()),
    );
    let t = now();
    RefreshTokenRepo::insert(&store, user.id, &a, t + Duration::days(30), t)
        .await
        .unwrap();
    RefreshTokenRepo::insert(&store, user.id, &b, t + Duration::days(30), t)
        .await
        .unwrap();

    let token_a = store.find_by_hash(&a).await.unwrap().unwrap();
    assert_eq!(token_a.user_id, user.id);
    assert!(token_a.revoked_at.is_none());
    assert!(store.revoke_if_active(token_a.id, t).await.unwrap());
    assert!(!store.revoke_if_active(token_a.id, t).await.unwrap());

    store.revoke_all_for_user(user.id, t).await.unwrap();
    let token_b = store.find_by_hash(&b).await.unwrap().unwrap();
    assert!(token_b.revoked_at.is_some());
    RefreshTokenRepo::delete(&store, token_b.id).await.unwrap();
    assert!(store.find_by_hash(&b).await.unwrap().is_none());
    assert!(store.find_by_hash("unknown").await.unwrap().is_none());
}

#[tokio::test]
async fn verification_tokens_are_single_use_and_expire() {
    let Some(store) = store().await else { return };
    let user = store
        .create(new_user(&unique_email()), now())
        .await
        .unwrap();
    let t = now();
    let (fresh, old) = (
        format!("v-{}", Uuid::new_v4()),
        format!("v-{}", Uuid::new_v4()),
    );
    assert_eq!(store.last_issued_at(user.id).await.unwrap(), None);
    store
        .replace_for_user(user.id, &old, t + Duration::hours(24), t)
        .await
        .unwrap();
    let later = t + Duration::minutes(5);
    store
        .replace_for_user(user.id, &fresh, later + Duration::hours(24), later)
        .await
        .unwrap();
    assert_eq!(store.last_issued_at(user.id).await.unwrap(), Some(later));

    assert_eq!(
        store.consume(&old, later).await.unwrap(),
        None,
        "replaced token is invalid"
    );
    assert_eq!(store.consume(&fresh, later).await.unwrap(), Some(user.id));
    assert_eq!(
        store.consume(&fresh, later).await.unwrap(),
        None,
        "single use"
    );

    let expiring = format!("v-{}", Uuid::new_v4());
    store
        .replace_for_user(user.id, &expiring, t + Duration::hours(24), t)
        .await
        .unwrap();
    assert_eq!(
        store
            .consume(&expiring, t + Duration::hours(25))
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn seeded_categories_are_listed_in_order() {
    let Some(store) = store().await else { return };
    let categories = store.list().await.unwrap();
    assert_eq!(categories[0].slug, "web-development");
    let found = CategoryRepo::find(&store, categories[0].id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.name, "Web Development");
}

#[tokio::test]
async fn instructor_profiles_upsert() {
    let Some(store) = store().await else { return };
    let user = store
        .create(new_user(&unique_email()), now())
        .await
        .unwrap();
    assert!(
        InstructorProfileRepo::find(&store, user.id)
            .await
            .unwrap()
            .is_none()
    );

    let mut profile = InstructorProfile {
        user_id: user.id,
        headline: "Rust dev".into(),
        bio: String::new(),
        website: None,
    };
    store.upsert(profile.clone()).await.unwrap();
    profile.website = Some("https://dara.dev".into());
    let saved = store.upsert(profile).await.unwrap();

    assert_eq!(saved.website.as_deref(), Some("https://dara.dev"));
    let found = InstructorProfileRepo::find(&store, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.headline, "Rust dev");
}

#[tokio::test]
async fn courses_round_trip_every_field() {
    let Some(store) = store().await else { return };
    let user = store
        .create(new_user(&unique_email()), now())
        .await
        .unwrap();
    let category = store.list().await.unwrap()[0].id;
    let mut course = Course::new_draft(
        Uuid::new_v4(),
        user.id,
        "Rust 101".into(),
        Some(category),
        now(),
    );
    CourseRepo::insert(&store, &course).await.unwrap();

    course.level = Level::Expert;
    course.currency = Currency::Khr;
    course.price_minor = 40_000;
    course.outcomes = vec!["One".into(), "Two".into()];
    course.status = CourseStatus::Rejected;
    course.rejection_reason = Some("Needs more content".into());
    course.thumbnail_key = Some("thumbnails/x.png".into());
    CourseRepo::save(&store, &course).await.unwrap();

    let found = CourseRepo::find(&store, course.id).await.unwrap().unwrap();
    assert_eq!(found.slug, course.slug);
    assert_eq!(found.level, Level::Expert);
    assert_eq!(found.currency, Currency::Khr);
    assert_eq!(found.price_minor, 40_000);
    assert_eq!(found.outcomes, vec!["One".to_string(), "Two".to_string()]);
    assert_eq!(found.status, CourseStatus::Rejected);
    assert_eq!(found.thumbnail_key.as_deref(), Some("thumbnails/x.png"));
    assert_eq!(store.list_by_instructor(user.id).await.unwrap().len(), 1);

    CourseRepo::delete(&store, course.id).await.unwrap();
    assert!(CourseRepo::find(&store, course.id).await.unwrap().is_none());
}

#[tokio::test]
async fn curriculum_reorders_atomically_and_cascades_deletes() {
    let Some(store) = store().await else { return };
    let user = store
        .create(new_user(&unique_email()), now())
        .await
        .unwrap();
    let course = Course::new_draft(Uuid::new_v4(), user.id, "Rust".into(), None, now());
    CourseRepo::insert(&store, &course).await.unwrap();

    let section = |title: &str, position| Section {
        id: Uuid::new_v4(),
        course_id: course.id,
        title: title.into(),
        position,
    };
    let (s1, s2) = (section("One", 0), section("Two", 1));
    store.insert_section(&s1).await.unwrap();
    store.insert_section(&s2).await.unwrap();
    let lecture = Lecture {
        id: Uuid::new_v4(),
        course_id: course.id,
        section_id: s1.id,
        title: "A".into(),
        kind: LectureKind::Article,
        position: 0,
        is_preview: true,
        article_body: "# Hi".into(),
        duration_sec: 0,
        video_asset_id: None,
    };
    store.insert_lecture(&lecture).await.unwrap();

    store
        .reorder(
            course.id,
            &[
                SectionOrder {
                    section_id: s2.id,
                    lecture_ids: vec![lecture.id],
                },
                SectionOrder {
                    section_id: s1.id,
                    lecture_ids: vec![],
                },
            ],
        )
        .await
        .unwrap();

    let sections = store.sections(course.id).await.unwrap();
    assert_eq!(
        sections
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>(),
        ["Two", "One"]
    );
    let moved = store.find_lecture(lecture.id).await.unwrap().unwrap();
    assert_eq!(moved.section_id, s2.id);
    assert_eq!(moved.kind, LectureKind::Article);
    assert!(moved.is_preview);

    store.delete_section(s2.id).await.unwrap();
    assert!(
        store.find_lecture(lecture.id).await.unwrap().is_none(),
        "lectures cascade"
    );
    assert_eq!(store.lectures(course.id).await.unwrap().len(), 0);
}

#[tokio::test]
async fn video_jobs_are_claimed_once_and_reclaimed_when_stale() {
    let Some(store) = store().await else { return };
    let user = store
        .create(new_user(&unique_email()), now())
        .await
        .unwrap();
    let course = Course::new_draft(Uuid::new_v4(), user.id, "Rust".into(), None, now());
    CourseRepo::insert(&store, &course).await.unwrap();
    let section = Section {
        id: Uuid::new_v4(),
        course_id: course.id,
        title: "S".into(),
        position: 0,
    };
    store.insert_section(&section).await.unwrap();
    let lecture = Lecture {
        id: Uuid::new_v4(),
        course_id: course.id,
        section_id: section.id,
        title: "V".into(),
        kind: LectureKind::Video,
        position: 0,
        is_preview: false,
        article_body: String::new(),
        duration_sec: 0,
        video_asset_id: None,
    };
    store.insert_lecture(&lecture).await.unwrap();

    // Other tests share the queue, so start from a time far in the past and only
    // look at our own asset.
    let t = now() - Duration::days(3650);
    let mut asset = VideoAsset {
        id: Uuid::new_v4(),
        owner_id: user.id,
        lecture_id: lecture.id,
        raw_key: "videos/raw/x.mp4".into(),
        status: VideoStatus::Processing,
        duration_sec: 0,
        error: None,
        playlists: Default::default(),
        attempts: 0,
        claimed_at: None,
        created_at: t,
        updated_at: t,
    };
    VideoAssetRepo::insert(&store, &asset).await.unwrap();

    let claimed = store
        .claim_next(t, t - Duration::minutes(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, asset.id);
    assert_eq!(claimed.attempts, 1);
    let again = store
        .claim_next(t, t - Duration::minutes(30))
        .await
        .unwrap();
    assert!(
        again.is_none_or(|a| a.id != asset.id),
        "a fresh claim is not handed out twice"
    );

    let later = t + Duration::hours(1);
    let reclaimed = store
        .claim_next(later, later - Duration::minutes(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reclaimed.id, asset.id);
    assert_eq!(reclaimed.attempts, 2);

    asset.status = VideoStatus::Ready;
    asset
        .playlists
        .insert("master.m3u8".into(), "#EXTM3U".into());
    VideoAssetRepo::save(&store, &asset).await.unwrap();
    let found = VideoAssetRepo::find(&store, asset.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.status, VideoStatus::Ready);
    assert_eq!(found.playlists["master.m3u8"], "#EXTM3U");
}

#[tokio::test]
async fn published_courses_can_be_searched_filtered_sorted_and_paged() {
    let Some(store) = store().await else { return };
    let user = store
        .create(new_user(&unique_email()), now())
        .await
        .unwrap();
    // A unique word keeps this test independent of other rows in the database.
    let tag = format!("zq{}", &Uuid::new_v4().simple().to_string()[..8]);
    let publish = |title: String, currency: Currency, price: i64, level: Level| {
        let mut c = Course::new_draft(Uuid::new_v4(), user.id, title, None, now());
        c.subtitle = tag.clone();
        c.currency = currency;
        c.price_minor = price;
        c.level = level;
        c.status = CourseStatus::Published;
        c.published_at = Some(now());
        c
    };
    let usd = publish(
        "Rust ten dollars".into(),
        Currency::Usd,
        1000,
        Level::Beginner,
    );
    let khr = publish("ភាសា Rust ខ្មែរ".into(), Currency::Khr, 20000, Level::Expert);
    let free = publish("Free intro".into(), Currency::Usd, 0, Level::Beginner);
    let mut draft = publish("Hidden draft".into(), Currency::Usd, 0, Level::All);
    draft.status = CourseStatus::Draft;
    for c in [&usd, &khr, &free, &draft] {
        CourseRepo::insert(&store, c).await.unwrap();
    }

    let filter = |sort, price, level| CatalogFilter {
        q: Some(tag.clone()),
        category_id: None,
        level,
        price,
        sort,
        limit: 10,
        offset: 0,
    };
    let titles =
        |r: (Vec<Course>, i64)| (r.0.into_iter().map(|c| c.title).collect::<Vec<_>>(), r.1);

    let (asc, total) = titles(
        store
            .search_published(&filter(CatalogSort::PriceAsc, None, None))
            .await
            .unwrap(),
    );
    assert_eq!(total, 3, "drafts are excluded");
    assert_eq!(
        asc,
        ["Free intro", "ភាសា Rust ខ្មែរ", "Rust ten dollars"],
        "KHR converted for sorting"
    );

    let (paid, _) = titles(
        store
            .search_published(&filter(
                CatalogSort::Newest,
                Some(PriceFilter::Paid),
                Some(Level::Beginner),
            ))
            .await
            .unwrap(),
    );
    assert_eq!(paid, ["Rust ten dollars"]);

    // Khmer substring (no word boundaries for full-text search) still matches.
    let mut khmer = filter(CatalogSort::Relevance, None, None);
    khmer.q = Some("ខ្មែរ".into());
    let (found, _) = titles(store.search_published(&khmer).await.unwrap());
    assert!(found.contains(&"ភាសា Rust ខ្មែរ".to_string()));

    let mut paged = filter(CatalogSort::PriceAsc, None, None);
    paged.limit = 1;
    paged.offset = 2;
    assert_eq!(
        titles(store.search_published(&paged).await.unwrap()),
        (vec!["Rust ten dollars".to_string()], 3)
    );

    assert!(
        store
            .find_published_by_slug(&usd.slug)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .find_published_by_slug(&draft.slug)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(store.find_many(&[user.id]).await.unwrap().len(), 1);
    assert!(
        store.stats(&[usd.id]).await.unwrap().is_empty(),
        "no lectures yet"
    );
}
