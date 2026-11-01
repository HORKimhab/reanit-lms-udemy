//! In-memory adapters for enrollments, carts, coupons, orders and payments.
#![allow(dead_code)]

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reanit_api::{
    error::{AppError, AppResult},
    modules::{
        cart::{SavedCourseRepo, SavedList},
        coupons::{Coupon, CouponRepo},
        courses::model::Currency,
        enrollments::{Enrollment, EnrollmentRepo},
        payments::{
            model::{Order, OrderStatus, Payment, PaymentAction, PaymentMethod, PaymentStatus},
            provider::{
                PaymentInit, PaymentProvider, PaymentProviders, PaymentRequest, ProviderKind,
                ProviderStatus,
            },
            repo::{MethodPatch, OrderRepo, PaymentMethodRepo, Settlement},
        },
    },
};
use uuid::Uuid;

#[derive(Default)]
pub struct MemEnrollments(pub Mutex<Vec<Enrollment>>);

#[async_trait]
impl EnrollmentRepo for MemEnrollments {
    async fn enroll(&self, e: &Enrollment) -> AppResult<()> {
        let mut all = self.0.lock().unwrap();
        if !all
            .iter()
            .any(|x| x.user_id == e.user_id && x.course_id == e.course_id)
        {
            all.push(e.clone());
        }
        Ok(())
    }

    async fn is_enrolled(&self, user_id: Uuid, course_id: Uuid) -> AppResult<bool> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.user_id == user_id && e.course_id == course_id))
    }

    async fn list_by_user(&self, user_id: Uuid) -> AppResult<Vec<Enrollment>> {
        let mut mine: Vec<Enrollment> = self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.user_id == user_id)
            .cloned()
            .collect();
        mine.sort_by_key(|e| std::cmp::Reverse(e.enrolled_at));
        Ok(mine)
    }
}

/// (list, user, course), most recently added first.
#[derive(Default)]
pub struct MemSavedCourses(Mutex<Vec<(SavedList, Uuid, Uuid)>>);

#[async_trait]
impl SavedCourseRepo for MemSavedCourses {
    async fn add(
        &self,
        list: SavedList,
        user_id: Uuid,
        course_id: Uuid,
        _at: DateTime<Utc>,
    ) -> AppResult<()> {
        let mut all = self.0.lock().unwrap();
        let entry = (list, user_id, course_id);
        if !all.contains(&entry) {
            all.insert(0, entry);
        }
        Ok(())
    }

    async fn remove(&self, list: SavedList, user_id: Uuid, course_id: Uuid) -> AppResult<()> {
        self.0
            .lock()
            .unwrap()
            .retain(|e| *e != (list, user_id, course_id));
        Ok(())
    }

    async fn list(&self, list: SavedList, user_id: Uuid) -> AppResult<Vec<Uuid>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(l, u, _)| *l == list && *u == user_id)
            .map(|(_, _, c)| *c)
            .collect())
    }
}

#[derive(Default)]
pub struct MemCoupons(pub Mutex<Vec<Coupon>>);

#[async_trait]
impl CouponRepo for MemCoupons {
    async fn insert(&self, c: &Coupon) -> AppResult<()> {
        let mut all = self.0.lock().unwrap();
        if all
            .iter()
            .any(|x| x.course_id == c.course_id && x.code == c.code)
        {
            return Err(AppError::Conflict("duplicate code".into()));
        }
        all.insert(0, c.clone());
        Ok(())
    }

    async fn list_by_course(&self, course_id: Uuid) -> AppResult<Vec<Coupon>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.course_id == course_id)
            .cloned()
            .collect())
    }

    async fn find_for(&self, course_ids: &[Uuid], codes: &[String]) -> AppResult<Vec<Coupon>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|c| course_ids.contains(&c.course_id) && codes.contains(&c.code))
            .cloned()
            .collect())
    }

    async fn delete(&self, course_id: Uuid, id: Uuid) -> AppResult<bool> {
        let mut all = self.0.lock().unwrap();
        let before = all.len();
        all.retain(|c| !(c.id == id && c.course_id == course_id));
        Ok(all.len() < before)
    }
}

/// Orders and payments; completing an order updates the sibling adapters the
/// way the Postgres transaction does.
pub struct MemOrders {
    pub orders: Mutex<Vec<Order>>,
    pub payments: Mutex<Vec<Payment>>,
    pub enrollments: Arc<MemEnrollments>,
    pub saved: Arc<MemSavedCourses>,
    pub coupons: Arc<MemCoupons>,
}

#[async_trait]
impl OrderRepo for MemOrders {
    async fn insert_order(&self, order: &Order) -> AppResult<()> {
        self.orders.lock().unwrap().insert(0, order.clone());
        Ok(())
    }

    async fn find_order(&self, id: Uuid) -> AppResult<Option<Order>> {
        Ok(self
            .orders
            .lock()
            .unwrap()
            .iter()
            .find(|o| o.id == id)
            .cloned())
    }

    async fn list_orders(&self, user_id: Uuid) -> AppResult<Vec<Order>> {
        Ok(self
            .orders
            .lock()
            .unwrap()
            .iter()
            .filter(|o| o.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn complete_order(
        &self,
        order_id: Uuid,
        settlement: Option<&Settlement>,
        at: DateTime<Utc>,
    ) -> AppResult<bool> {
        if let Some(s) = settlement {
            let mut payments = self.payments.lock().unwrap();
            if let Some(p) = payments
                .iter_mut()
                .find(|p| p.id == s.payment_id && p.status == PaymentStatus::Pending)
            {
                p.status = PaymentStatus::Paid;
                p.provider_tran_id = Some(s.provider_tran_id.clone());
                p.raw_response = Some(s.raw.clone());
                p.paid_at = Some(at);
            }
        }
        let order = {
            let mut orders = self.orders.lock().unwrap();
            let Some(order) = orders
                .iter_mut()
                .find(|o| o.id == order_id && o.status == OrderStatus::Pending)
            else {
                return Ok(false);
            };
            order.status = OrderStatus::Paid;
            order.paid_at = Some(at);
            order.clone()
        };
        for item in &order.items {
            self.enrollments
                .enroll(&Enrollment {
                    user_id: order.user_id,
                    course_id: item.course_id,
                    order_id: Some(order.id),
                    enrolled_at: at,
                })
                .await?;
            self.saved
                .remove(SavedList::Cart, order.user_id, item.course_id)
                .await?;
            if let Some(coupon_id) = item.coupon_id {
                let mut coupons = self.coupons.0.lock().unwrap();
                if let Some(c) = coupons.iter_mut().find(|c| c.id == coupon_id) {
                    c.used_count += 1;
                }
            }
        }
        Ok(true)
    }

    async fn insert_payment(&self, payment: &Payment) -> AppResult<()> {
        self.payments.lock().unwrap().insert(0, payment.clone());
        Ok(())
    }

    async fn find_payment_by_reference(
        &self,
        provider: ProviderKind,
        reference: &str,
    ) -> AppResult<Option<Payment>> {
        Ok(self
            .payments
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.provider == provider && p.reference == reference)
            .cloned())
    }

    async fn list_payments(&self, order_id: Uuid) -> AppResult<Vec<Payment>> {
        Ok(self
            .payments
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.order_id == order_id)
            .cloned()
            .collect())
    }

    async fn list_pending_payments(&self, limit: i64) -> AppResult<Vec<Payment>> {
        let mut pending: Vec<Payment> = self
            .payments
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.status == PaymentStatus::Pending)
            .cloned()
            .collect();
        pending.reverse();
        pending.truncate(limit as usize);
        Ok(pending)
    }

    async fn close_payment(
        &self,
        id: Uuid,
        status: PaymentStatus,
        reason: Option<&str>,
        raw: Option<&serde_json::Value>,
    ) -> AppResult<()> {
        let mut payments = self.payments.lock().unwrap();
        if let Some(p) = payments
            .iter_mut()
            .find(|p| p.id == id && p.status == PaymentStatus::Pending)
        {
            p.status = status;
            p.failure_reason = reason.map(String::from);
            if let Some(raw) = raw {
                p.raw_response = Some(raw.clone());
            }
        }
        Ok(())
    }

    async fn touch_payment(&self, id: Uuid, checked_at: DateTime<Utc>) -> AppResult<()> {
        if let Some(p) = self
            .payments
            .lock()
            .unwrap()
            .iter_mut()
            .find(|p| p.id == id)
        {
            p.last_checked_at = Some(checked_at);
        }
        Ok(())
    }
}

/// Same rows as the migration seed.
pub struct MemPaymentMethods(Mutex<Vec<PaymentMethod>>);

impl Default for MemPaymentMethods {
    fn default() -> Self {
        let method = |code: &str, provider, label: &str, currencies: Vec<Currency>, sort_order| {
            PaymentMethod {
                code: code.into(),
                provider,
                label: label.into(),
                description: String::new(),
                enabled: true,
                currencies,
                sort_order,
            }
        };
        Self(Mutex::new(vec![
            method(
                "bakong_khqr",
                ProviderKind::Bakong,
                "KHQR",
                vec![Currency::Usd, Currency::Khr],
                10,
            ),
            method(
                "aba_khqr",
                ProviderKind::AbaPayway,
                "ABA KHQR",
                vec![Currency::Usd, Currency::Khr],
                20,
            ),
            method(
                "aba_cards",
                ProviderKind::AbaPayway,
                "Credit / debit card",
                vec![Currency::Usd],
                30,
            ),
        ]))
    }
}

#[async_trait]
impl PaymentMethodRepo for MemPaymentMethods {
    async fn list_methods(&self) -> AppResult<Vec<PaymentMethod>> {
        let mut all = self.0.lock().unwrap().clone();
        all.sort_by(|a, b| (a.sort_order, &a.code).cmp(&(b.sort_order, &b.code)));
        Ok(all)
    }

    async fn update_method(&self, code: &str, p: &MethodPatch) -> AppResult<Option<PaymentMethod>> {
        let mut all = self.0.lock().unwrap();
        let Some(m) = all.iter_mut().find(|m| m.code == code) else {
            return Ok(None);
        };
        if let Some(v) = p.enabled {
            m.enabled = v;
        }
        if let Some(v) = &p.label {
            m.label = v.clone();
        }
        if let Some(v) = &p.description {
            m.description = v.clone();
        }
        if let Some(v) = p.sort_order {
            m.sort_order = v;
        }
        if let Some(v) = &p.currencies {
            m.currencies = v.clone();
        }
        Ok(Some(m.clone()))
    }
}

// ---------- scripted payment providers ----------

#[derive(Default)]
struct FakeShared {
    unconfigured: Vec<ProviderKind>,
    requests: Vec<(ProviderKind, PaymentRequest)>,
    actions: Vec<(ProviderKind, PaymentAction)>,
    checks: HashMap<ProviderKind, usize>,
    /// reference -> what the provider reports as paid.
    settled: HashMap<String, (i64, Currency)>,
}

/// Every provider "configured", answering from a script the test controls.
#[derive(Clone, Default)]
pub struct FakePayments(Arc<Mutex<FakeShared>>);

impl FakePayments {
    pub fn unconfigure(&self, kind: ProviderKind) {
        self.0.lock().unwrap().unconfigured.push(kind);
    }

    pub fn requests(&self, kind: ProviderKind) -> Vec<PaymentRequest> {
        self.0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|(k, _)| *k == kind)
            .map(|(_, r)| r.clone())
            .collect()
    }

    pub fn last_action(&self, kind: ProviderKind) -> Option<PaymentAction> {
        self.0
            .lock()
            .unwrap()
            .actions
            .iter()
            .rev()
            .find(|(k, _)| *k == kind)
            .map(|(_, a)| a.clone())
    }

    pub fn checks(&self, kind: ProviderKind) -> usize {
        self.0
            .lock()
            .unwrap()
            .checks
            .get(&kind)
            .copied()
            .unwrap_or(0)
    }

    /// From now on the provider reports this payment as paid.
    pub fn settle(
        &self,
        _kind: ProviderKind,
        reference: &str,
        amount_minor: i64,
        currency: Currency,
    ) {
        self.0
            .lock()
            .unwrap()
            .settled
            .insert(reference.into(), (amount_minor, currency));
    }
}

impl PaymentProviders for FakePayments {
    fn get(&self, kind: ProviderKind) -> Option<Arc<dyn PaymentProvider>> {
        if self.0.lock().unwrap().unconfigured.contains(&kind) {
            return None;
        }
        Some(Arc::new(FakeProvider {
            kind,
            shared: self.0.clone(),
        }))
    }
}

struct FakeProvider {
    kind: ProviderKind,
    shared: Arc<Mutex<FakeShared>>,
}

#[async_trait]
impl PaymentProvider for FakeProvider {
    async fn create(&self, req: &PaymentRequest) -> AppResult<PaymentInit> {
        let reference = format!("ref-{}", &req.payment_id.simple().to_string()[..12]);
        let action = if req.method == "aba_cards" {
            PaymentAction::Redirect {
                url: "https://pay.test/checkout".into(),
                fields: BTreeMap::from([("tran_id".to_string(), reference.clone())]),
            }
        } else {
            PaymentAction::Qr {
                qr_string: format!("000201010212{reference}"),
                deeplink: (self.kind == ProviderKind::AbaPayway)
                    .then(|| format!("abamobilebank://pay?{reference}")),
            }
        };
        let mut shared = self.shared.lock().unwrap();
        shared.requests.push((self.kind, req.clone()));
        shared.actions.push((self.kind, action.clone()));
        Ok(PaymentInit {
            reference,
            action,
            raw: serde_json::json!({ "fake": true }),
        })
    }

    async fn check(&self, payment: &Payment) -> AppResult<ProviderStatus> {
        let mut shared = self.shared.lock().unwrap();
        *shared.checks.entry(self.kind).or_default() += 1;
        Ok(match shared.settled.get(&payment.reference) {
            Some((amount_minor, currency)) => ProviderStatus::Paid {
                provider_tran_id: format!("tx-{}", payment.reference),
                amount_minor: *amount_minor,
                currency: *currency,
                raw: serde_json::json!({ "fake": "paid" }),
            },
            None => ProviderStatus::Pending,
        })
    }
}
