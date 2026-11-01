use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Generates `as_str` / `FromStr` for a string-backed enum stored as TEXT.
macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }
        impl std::str::FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                match s {
                    $($text => Ok($name::$variant),)+
                    other => Err(format!("unknown {}: {other}", stringify!($name))),
                }
            }
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CourseStatus {
    Draft,
    /// Submitted, waiting for admin review. Locked for editing.
    Pending,
    Published,
    Rejected,
}
text_enum!(CourseStatus { Draft => "draft", Pending => "pending", Published => "published", Rejected => "rejected" });

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    All,
    Beginner,
    Intermediate,
    Expert,
}
text_enum!(Level { All => "all", Beginner => "beginner", Intermediate => "intermediate", Expert => "expert" });

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Currency {
    #[serde(rename = "USD")]
    Usd,
    #[serde(rename = "KHR")]
    Khr,
}
text_enum!(Currency { Usd => "USD", Khr => "KHR" });

#[derive(Clone, Debug, Serialize)]
pub struct Course {
    pub id: Uuid,
    pub instructor_id: Uuid,
    pub category_id: Option<Uuid>,
    pub title: String,
    /// Set once at creation so public URLs stay stable when the title changes.
    pub slug: String,
    pub subtitle: String,
    pub description: String,
    pub level: Level,
    pub language: String,
    /// Price in the currency's minor unit (USD cents, KHR riel). 0 = free.
    pub price_minor: i64,
    pub currency: Currency,
    pub thumbnail_key: Option<String>,
    pub outcomes: Vec<String>,
    pub requirements: Vec<String>,
    pub status: CourseStatus,
    pub rejection_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

impl Course {
    pub fn new_draft(
        id: Uuid,
        instructor_id: Uuid,
        title: String,
        category_id: Option<Uuid>,
        now: DateTime<Utc>,
    ) -> Self {
        let slug = format!("{}-{}", slugify(&title), &id.simple().to_string()[..6]);
        Self {
            id,
            instructor_id,
            category_id,
            title,
            slug,
            subtitle: String::new(),
            description: String::new(),
            level: Level::All,
            language: "en".into(),
            price_minor: 0,
            currency: Currency::Usd,
            thumbnail_key: None,
            outcomes: vec![],
            requirements: vec![],
            status: CourseStatus::Draft,
            rejection_reason: None,
            created_at: now,
            updated_at: now,
            published_at: None,
        }
    }
}

/// API representation: the course plus derived URLs.
#[derive(Debug, Serialize)]
pub struct CourseDto {
    #[serde(flatten)]
    pub course: Course,
    pub thumbnail_url: Option<String>,
}

/// Lowercase ASCII words joined by `-`; non-ASCII titles fall back to "course".
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "course".into()
    } else {
        slug.chars().take(60).collect()
    }
}
