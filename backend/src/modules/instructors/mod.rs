use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Serialize)]
pub struct InstructorProfile {
    pub user_id: Uuid,
    pub headline: String,
    pub bio: String,
    pub website: Option<String>,
}

#[async_trait]
pub trait InstructorProfileRepo: Send + Sync {
    async fn find(&self, user_id: Uuid) -> AppResult<Option<InstructorProfile>>;
    async fn upsert(&self, profile: InstructorProfile) -> AppResult<InstructorProfile>;
}

/// Request body shared by "become instructor" and "update profile".
#[derive(Deserialize)]
pub struct ProfileInput {
    pub headline: String,
    #[serde(default)]
    pub bio: String,
    #[serde(default)]
    pub website: Option<String>,
}

impl ProfileInput {
    pub fn validate(self, user_id: Uuid) -> AppResult<InstructorProfile> {
        let headline = self.headline.trim().to_string();
        if headline.is_empty() || headline.chars().count() > 60 {
            return Err(AppError::BadRequest(
                "headline must be 1-60 characters".into(),
            ));
        }
        let bio = self.bio.trim().to_string();
        if bio.chars().count() > 5000 {
            return Err(AppError::BadRequest(
                "bio must be at most 5000 characters".into(),
            ));
        }
        let website = self
            .website
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty());
        if let Some(w) = &website
            && !(w.starts_with("https://") || w.starts_with("http://"))
        {
            return Err(AppError::BadRequest(
                "website must start with http:// or https://".into(),
            ));
        }
        Ok(InstructorProfile {
            user_id,
            headline,
            bio,
            website,
        })
    }
}
