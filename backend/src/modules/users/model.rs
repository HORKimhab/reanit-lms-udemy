use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Student,
    Instructor,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Student => "student",
            Role::Instructor => "instructor",
            Role::Admin => "admin",
        }
    }
}

impl std::str::FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "student" => Ok(Role::Student),
            "instructor" => Ok(Role::Instructor),
            "admin" => Ok(Role::Admin),
            other => Err(format!("unknown role: {other}")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub role: Role,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn is_email_verified(&self) -> bool {
        self.email_verified_at.is_some()
    }
}

#[derive(Clone, Debug)]
pub struct NewUser {
    pub name: String,
    pub email: String,
    pub password_hash: String,
}

/// Public representation returned by the API (never includes the password hash).
#[derive(Debug, Serialize)]
pub struct UserDto {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: Role,
    pub email_verified: bool,
    pub avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<&User> for UserDto {
    fn from(u: &User) -> Self {
        Self {
            id: u.id,
            name: u.name.clone(),
            email: u.email.clone(),
            role: u.role,
            email_verified: u.is_email_verified(),
            avatar_url: u.avatar_url.clone(),
            created_at: u.created_at,
        }
    }
}
