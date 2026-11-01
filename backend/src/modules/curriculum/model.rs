use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LectureKind {
    Video,
    Article,
}

impl LectureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LectureKind::Video => "video",
            LectureKind::Article => "article",
        }
    }
}

impl std::str::FromStr for LectureKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "video" => Ok(LectureKind::Video),
            "article" => Ok(LectureKind::Article),
            other => Err(format!("unknown lecture kind: {other}")),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Section {
    pub id: Uuid,
    pub course_id: Uuid,
    pub title: String,
    pub position: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Lecture {
    pub id: Uuid,
    pub course_id: Uuid,
    pub section_id: Uuid,
    pub title: String,
    pub kind: LectureKind,
    pub position: i32,
    /// Free preview, watchable without enrolling.
    pub is_preview: bool,
    /// Markdown body for article lectures.
    pub article_body: String,
    pub duration_sec: i32,
    pub video_asset_id: Option<Uuid>,
}

/// A section with its lectures, both in display order.
#[derive(Debug, Serialize)]
pub struct SectionWithLectures {
    #[serde(flatten)]
    pub section: Section,
    pub lectures: Vec<Lecture>,
}

/// Groups lectures under their sections, ordering both by position.
pub fn assemble(
    mut sections: Vec<Section>,
    mut lectures: Vec<Lecture>,
) -> Vec<SectionWithLectures> {
    sections.sort_by_key(|s| s.position);
    lectures.sort_by_key(|l| l.position);
    sections
        .into_iter()
        .map(|section| {
            let lectures = lectures
                .iter()
                .filter(|l| l.section_id == section.id)
                .cloned()
                .collect();
            SectionWithLectures { section, lectures }
        })
        .collect()
}
