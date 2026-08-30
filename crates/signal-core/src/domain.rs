use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    Feed { url: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub category: String,
    pub enabled: bool,
    pub weight: f64,
    pub kind: SourceKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceOrigin {
    Standard,
    Personal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceRecord {
    pub source: Source,
    pub origin: SourceOrigin,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Candidate {
    pub source_id: String,
    pub external_id: String,
    pub canonical_url: String,
    pub title: String,
    pub excerpt: String,
    pub published_at: Option<DateTime<Utc>>,
    pub collected_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScoreBreakdown {
    pub recency: f64,
    pub source_weight: f64,
    pub corroboration: f64,
    pub total: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Story {
    pub id: String,
    pub title: String,
    pub canonical_url: String,
    pub excerpt: String,
    pub category: String,
    pub published_at: Option<DateTime<Utc>>,
    pub source_ids: Vec<String>,
    pub score: ScoreBreakdown,
    pub smart_summary: String,
    pub is_read: bool,
    pub is_saved: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BriefingItem {
    pub position: u32,
    pub section: String,
    #[serde(default)]
    pub is_stale: bool,
    pub story: Story,
    #[serde(default)]
    pub selected_summary: Option<crate::SummaryVariant>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Briefing {
    pub date: NaiveDate,
    pub generated_at: DateTime<Utc>,
    pub items: Vec<BriefingItem>,
}
