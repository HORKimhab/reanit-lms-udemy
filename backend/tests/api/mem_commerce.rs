//! In-memory adapters for enrollments, carts, coupons, orders and payments.
#![allow(dead_code)]

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reanit_api::{
    error::AppResult,
    modules::{
        cart::{SavedCourseRepo, SavedList},
        enrollments::{Enrollment, EnrollmentRepo},
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
