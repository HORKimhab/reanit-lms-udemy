use std::sync::Arc;

use crate::{
    auth::store::{RefreshTokenRepo, VerificationTokenRepo},
    config::Config,
    infra::{clock::Clock, health::HealthProbe, mailer::Mailer},
    modules::users::repo::UserRepo,
};

/// Everything handlers depend on. Infrastructure sits behind traits so the
/// whole HTTP API can run against in-memory adapters in tests.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub users: Arc<dyn UserRepo>,
    pub refresh_tokens: Arc<dyn RefreshTokenRepo>,
    pub verification_tokens: Arc<dyn VerificationTokenRepo>,
    pub mailer: Arc<dyn Mailer>,
    pub clock: Arc<dyn Clock>,
    pub health: Arc<dyn HealthProbe>,
}
