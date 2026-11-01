use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{
    auth::tokens,
    error::AppError,
    modules::users::model::{Role, User},
    state::AppState,
};

/// The authenticated user, from an `Authorization: Bearer <access token>` header.
/// The user is re-loaded on every request so role changes apply immediately.
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        let user_id =
            tokens::verify_access_token(&state.config.jwt_secret, token, state.clock.now())?;
        let user = state
            .users
            .find_by_id(user_id)
            .await?
            .ok_or(AppError::Unauthorized)?;
        Ok(CurrentUser(user))
    }
}

impl CurrentUser {
    pub fn require_role(&self, role: Role) -> Result<&User, AppError> {
        if self.0.role == role {
            Ok(&self.0)
        } else {
            Err(AppError::Forbidden)
        }
    }

    pub fn require_verified_email(&self) -> Result<&User, AppError> {
        if self.0.is_email_verified() {
            Ok(&self.0)
        } else {
            Err(AppError::Forbidden)
        }
    }
}

/// The user if a bearer token is present. A *missing* header yields `None`;
/// a present but invalid/expired token is still rejected (401) so clients
/// know to refresh their session.
pub struct OptionalUser(pub Option<User>);

impl FromRequestParts<AppState> for OptionalUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        if !parts
            .headers
            .contains_key(axum::http::header::AUTHORIZATION)
        {
            return Ok(OptionalUser(None));
        }
        CurrentUser::from_request_parts(parts, state)
            .await
            .map(|CurrentUser(u)| OptionalUser(Some(u)))
    }
}
