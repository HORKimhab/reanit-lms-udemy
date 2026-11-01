use std::sync::Arc;

use crate::{
    auth::store::{RefreshTokenRepo, VerificationTokenRepo},
    config::Config,
    infra::{clock::Clock, health::HealthProbe, mailer::Mailer, storage::Storage},
    modules::{
        categories::CategoryRepo, courses::repo::CourseRepo, curriculum::repo::CurriculumRepo,
        enrollments::EnrollmentRepo, instructors::InstructorProfileRepo, users::repo::UserRepo,
        videos::repo::VideoAssetRepo,
    },
};

/// Everything handlers depend on. Infrastructure sits behind traits so the
/// whole HTTP API can run against in-memory adapters in tests.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub users: Arc<dyn UserRepo>,
    pub categories: Arc<dyn CategoryRepo>,
    pub instructor_profiles: Arc<dyn InstructorProfileRepo>,
    pub courses: Arc<dyn CourseRepo>,
    pub curriculum: Arc<dyn CurriculumRepo>,
    pub video_assets: Arc<dyn VideoAssetRepo>,
    pub enrollments: Arc<dyn EnrollmentRepo>,
    pub refresh_tokens: Arc<dyn RefreshTokenRepo>,
    pub verification_tokens: Arc<dyn VerificationTokenRepo>,
    pub mailer: Arc<dyn Mailer>,
    pub clock: Arc<dyn Clock>,
    pub health: Arc<dyn HealthProbe>,
    pub storage: Arc<Storage>,
}
