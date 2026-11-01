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
    assert!(
        store
            .find_by_hash(&b)
            .await
            .unwrap()
            .unwrap()
            .revoked_at
            .is_some()
    );
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
    VerificationTokenRepo::insert(&store, user.id, &fresh, t + Duration::hours(24), t)
        .await
        .unwrap();
    VerificationTokenRepo::insert(&store, user.id, &old, t + Duration::hours(24), t)
        .await
        .unwrap();

    assert_eq!(store.consume(&fresh, t).await.unwrap(), Some(user.id));
    assert_eq!(store.consume(&fresh, t).await.unwrap(), None);
    assert_eq!(
        store.consume(&old, t + Duration::hours(25)).await.unwrap(),
        None
    );
}
