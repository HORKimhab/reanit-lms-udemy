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

#[tokio::test]
async fn cart_and_wishlist_are_separate_newest_first_lists() {
    use reanit_api::modules::cart::{SavedCourseRepo, SavedList};
    let Some(store) = store().await else { return };
    let (user, a) = user_and_course(&store, 100).await;
    let (_, b) = user_and_course(&store, 100).await;
    let t0 = now();

    store.add(SavedList::Cart, user, a.id, t0).await.unwrap();
    store
        .add(SavedList::Cart, user, b.id, t0 + Duration::seconds(1))
        .await
        .unwrap();
    store
        .add(SavedList::Cart, user, a.id, t0 + Duration::seconds(2))
        .await
        .unwrap();
    store
        .add(SavedList::Wishlist, user, a.id, t0)
        .await
        .unwrap();

    assert_eq!(
        store.list(SavedList::Cart, user).await.unwrap(),
        [b.id, a.id]
    );
    store.remove(SavedList::Cart, user, b.id).await.unwrap();
    assert_eq!(store.list(SavedList::Cart, user).await.unwrap(), [a.id]);
    assert_eq!(store.list(SavedList::Wishlist, user).await.unwrap(), [a.id]);
}

#[tokio::test]
async fn coupon_codes_are_unique_per_course_and_found_by_code() {
    use reanit_api::{
        error::AppError,
        modules::coupons::{Coupon, CouponRepo},
    };
    let Some(store) = store().await else { return };
    let (_, course) = user_and_course(&store, 1000).await;
    let coupon = |code: &str| Coupon {
        id: Uuid::new_v4(),
        course_id: course.id,
        code: code.into(),
        discount_percent: 25,
        expires_at: Some(now() + Duration::days(1)),
        max_uses: Some(3),
        used_count: 0,
        created_at: now(),
    };
    let launch = coupon("LAUNCH");
    CouponRepo::insert(&store, &launch).await.unwrap();
    assert!(matches!(
        CouponRepo::insert(&store, &coupon("LAUNCH")).await,
        Err(AppError::Conflict(_))
    ));

    let found = store
        .find_for(&[course.id], &["LAUNCH".into(), "OTHER".into()])
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].expires_at, launch.expires_at);
    assert_eq!(found[0].max_uses, Some(3));

    assert!(
        CouponRepo::delete(&store, course.id, launch.id)
            .await
            .unwrap()
    );
    assert!(store.list_by_course(course.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn completing_an_order_enrolls_clears_cart_and_counts_coupons_once() {
    use reanit_api::modules::{
        cart::{SavedCourseRepo, SavedList},
        coupons::{Coupon, CouponRepo},
        courses::model::Currency,
        payments::{
            model::{Order, OrderItem, OrderStatus, Payment, PaymentAction, PaymentStatus},
            provider::ProviderKind,
            repo::{OrderRepo, Settlement},
        },
    };
    let Some(store) = store().await else { return };
    let (_, course) = user_and_course(&store, 2000).await;
    let buyer = store
        .create(new_user(&unique_email()), now())
        .await
        .unwrap()
        .id;
    let coupon = Coupon {
        id: Uuid::new_v4(),
        course_id: course.id,
        code: "TEN".into(),
        discount_percent: 10,
        expires_at: None,
        max_uses: None,
        used_count: 0,
        created_at: now(),
    };
    CouponRepo::insert(&store, &coupon).await.unwrap();
    store
        .add(SavedList::Cart, buyer, course.id, now())
        .await
        .unwrap();

    let order = Order {
        id: Uuid::new_v4(),
        user_id: buyer,
        status: OrderStatus::Pending,
        currency: Currency::Usd,
        subtotal_minor: 2000,
        total_minor: 1800,
        items: vec![OrderItem {
            course_id: course.id,
            title: "Rust".into(),
            list_price_minor: 2000,
            price_minor: 1800,
            coupon_id: Some(coupon.id),
            coupon_code: Some("TEN".into()),
        }],
        created_at: now(),
        paid_at: None,
    };
    store.insert_order(&order).await.unwrap();
    let payment = Payment {
        id: Uuid::new_v4(),
        order_id: order.id,
        method: "bakong_khqr".into(),
        provider: ProviderKind::Bakong,
        amount_minor: 1800,
        currency: Currency::Usd,
        status: PaymentStatus::Pending,
        reference: format!("md5-{}", Uuid::new_v4()),
        provider_tran_id: None,
        action: PaymentAction::Qr {
            qr_string: "000201".into(),
            deeplink: None,
        },
        failure_reason: None,
        raw_response: None,
        expires_at: now() + Duration::minutes(10),
        last_checked_at: None,
        created_at: now(),
        paid_at: None,
    };
    store.insert_payment(&payment).await.unwrap();
    assert!(
        store
            .list_pending_payments(1000)
            .await
            .unwrap()
            .iter()
            .any(|p| p.id == payment.id)
    );
    let found = store
        .find_payment_by_reference(ProviderKind::Bakong, &payment.reference)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.action, payment.action);

    let settlement = Settlement {
        payment_id: payment.id,
        provider_tran_id: format!("tx-{}", Uuid::new_v4()),
        raw: serde_json::json!({ "ok": true }),
    };
    let paid_at = now();
    assert!(
        store
            .complete_order(order.id, Some(&settlement), paid_at)
            .await
            .unwrap()
    );
    assert!(
        !store
            .complete_order(order.id, Some(&settlement), paid_at)
            .await
            .unwrap(),
        "second completion is a no-op"
    );

    let order = store.find_order(order.id).await.unwrap().unwrap();
    assert_eq!(order.status, OrderStatus::Paid);
    assert_eq!(order.paid_at, Some(paid_at));
    assert_eq!(order.items[0].coupon_code.as_deref(), Some("TEN"));
    let payment = &store.list_payments(order.id).await.unwrap()[0];
    assert_eq!(payment.status, PaymentStatus::Paid);
    assert_eq!(payment.provider_tran_id, Some(settlement.provider_tran_id));
    assert!(store.is_enrolled(buyer, course.id).await.unwrap());
    assert!(store.list(SavedList::Cart, buyer).await.unwrap().is_empty());
    assert_eq!(
        store.list_by_course(course.id).await.unwrap()[0].used_count,
        1
    );
    assert_eq!(store.list_orders(buyer).await.unwrap().len(), 1);
}

#[tokio::test]
async fn pending_payments_can_be_closed_once() {
    use reanit_api::modules::{
        courses::model::Currency,
        payments::{
            model::{Order, OrderStatus, Payment, PaymentAction, PaymentStatus},
            provider::ProviderKind,
            repo::OrderRepo,
        },
    };
    let Some(store) = store().await else { return };
    let (buyer, _) = user_and_course(&store, 100).await;
    let order = Order {
        id: Uuid::new_v4(),
        user_id: buyer,
        status: OrderStatus::Pending,
        currency: Currency::Khr,
        subtotal_minor: 4000,
        total_minor: 4000,
        items: vec![],
        created_at: now(),
        paid_at: None,
    };
    store.insert_order(&order).await.unwrap();
    let payment = Payment {
        id: Uuid::new_v4(),
        order_id: order.id,
        method: "aba_khqr".into(),
        provider: ProviderKind::AbaPayway,
        amount_minor: 4000,
        currency: Currency::Khr,
        status: PaymentStatus::Pending,
        reference: format!("tran-{}", &Uuid::new_v4().simple().to_string()[..12]),
        provider_tran_id: None,
        action: PaymentAction::Redirect {
            url: "https://pay.test".into(),
            fields: [("a".to_string(), "b".to_string())].into(),
        },
        failure_reason: None,
        raw_response: None,
        expires_at: now(),
        last_checked_at: None,
        created_at: now(),
        paid_at: None,
    };
    store.insert_payment(&payment).await.unwrap();

    let checked = now();
    store.touch_payment(payment.id, checked).await.unwrap();
    store
        .close_payment(payment.id, PaymentStatus::Expired, None, None)
        .await
        .unwrap();
    store
        .close_payment(payment.id, PaymentStatus::Failed, Some("late"), None)
        .await
        .unwrap();

    let after = &store.list_payments(order.id).await.unwrap()[0];
    assert_eq!(after.status, PaymentStatus::Expired, "closing is one-way");
    assert_eq!(after.last_checked_at, Some(checked));
    assert_eq!(after.action, payment.action);
}

#[tokio::test]
async fn payment_methods_are_seeded_and_patchable() {
    use reanit_api::modules::{
        courses::model::Currency,
        payments::repo::{MethodPatch, PaymentMethodRepo},
    };
    let Some(store) = store().await else { return };
    let codes: Vec<String> = store
        .list_methods()
        .await
        .unwrap()
        .into_iter()
        .map(|m| m.code)
        .collect();
    for code in ["bakong_khqr", "aba_khqr", "aba_cards"] {
        assert!(codes.contains(&code.to_string()));
    }

    let patched = store
        .update_method(
            "aba_cards",
            &MethodPatch {
                currencies: Some(vec![Currency::Usd, Currency::Khr]),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(patched.currencies, [Currency::Usd, Currency::Khr]);
    store
        .update_method(
            "aba_cards",
            &MethodPatch {
                currencies: Some(vec![Currency::Usd]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(
        store
            .update_method("nope", &MethodPatch::default())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn stale_orders_expire_unless_a_payment_is_in_progress() {
    use reanit_api::modules::{
        courses::model::Currency,
        payments::{
            model::{Order, OrderStatus, Payment, PaymentAction, PaymentStatus},
            provider::ProviderKind,
            repo::OrderRepo,
        },
    };
    let Some(store) = store().await else { return };
    let (buyer, _) = user_and_course(&store, 100).await;
    let long_ago = now() - Duration::days(2);
    let order = |created_at| Order {
        id: Uuid::new_v4(),
        user_id: buyer,
        status: OrderStatus::Pending,
        currency: Currency::Usd,
        subtotal_minor: 100,
        total_minor: 100,
        items: vec![],
        created_at,
        paid_at: None,
    };
    let abandoned = order(long_ago);
    let paying = order(long_ago);
    let fresh = order(now());
    for o in [&abandoned, &paying, &fresh] {
        store.insert_order(o).await.unwrap();
    }
    store
        .insert_payment(&Payment {
            id: Uuid::new_v4(),
            order_id: paying.id,
            method: "bakong_khqr".into(),
            provider: ProviderKind::Bakong,
            amount_minor: 100,
            currency: Currency::Usd,
            status: PaymentStatus::Pending,
            reference: format!("md5-{}", Uuid::new_v4()),
            provider_tran_id: None,
            action: PaymentAction::Qr {
                qr_string: "000201".into(),
                deeplink: None,
            },
            failure_reason: None,
            raw_response: None,
            expires_at: now() + Duration::minutes(10),
            last_checked_at: None,
            created_at: now(),
            paid_at: None,
        })
        .await
        .unwrap();

    let expired = store
        .expire_stale_orders(now() - Duration::days(1))
        .await
        .unwrap();

    assert!(expired >= 1);
    let status = |id| {
        let store = &store;
        async move { store.find_order(id).await.unwrap().unwrap().status }
    };
    assert_eq!(status(abandoned.id).await, OrderStatus::Expired);
    assert_eq!(status(paying.id).await, OrderStatus::Pending);
    assert_eq!(status(fresh.id).await, OrderStatus::Pending);
}
