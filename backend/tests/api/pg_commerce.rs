//! Postgres round-trips for the commerce adapters (skipped without `DATABASE_URL`).

use chrono::Duration;
use reanit_api::{
    infra::postgres::PgStore,
    modules::{
        courses::{model::Course, repo::CourseRepo},
        enrollments::{Enrollment, EnrollmentRepo},
        users::repo::UserRepo,
    },
};
use uuid::Uuid;

use crate::pg_store::{new_user, now, store, unique_email};

pub async fn user_and_course(store: &PgStore, price_minor: i64) -> (Uuid, Course) {
    let user = store
        .create(new_user(&unique_email()), now())
        .await
        .unwrap();
    let mut course = Course::new_draft(Uuid::new_v4(), user.id, "Rust".into(), None, now());
    course.price_minor = price_minor;
    CourseRepo::insert(store, &course).await.unwrap();
    (user.id, course)
}

#[tokio::test]
async fn enrollments_are_idempotent_and_listed_newest_first() {
    let Some(store) = store().await else { return };
    let (user, first) = user_and_course(&store, 0).await;
    let (_, second) = user_and_course(&store, 0).await;
    let enroll = |course: Uuid, at| Enrollment {
        user_id: user,
        course_id: course,
        order_id: None,
        enrolled_at: at,
    };

    let t0 = now();
    store.enroll(&enroll(first.id, t0)).await.unwrap();
    store
        .enroll(&enroll(second.id, t0 + Duration::seconds(5)))
        .await
        .unwrap();
    store
        .enroll(&enroll(first.id, t0 + Duration::seconds(9)))
        .await
        .unwrap();

    let mine = store.list_by_user(user).await.unwrap();
    let ids: Vec<Uuid> = mine.iter().map(|e| e.course_id).collect();
    assert_eq!(ids, [second.id, first.id]);
    assert_eq!(mine[1].enrolled_at, t0, "first enrollment is kept");
    assert!(store.is_enrolled(user, first.id).await.unwrap());
    assert!(!store.is_enrolled(Uuid::new_v4(), first.id).await.unwrap());
}
