mod config;
mod domain;
mod error;
mod paths;
mod storage;

pub use config::{AppConfig, BriefingConfig, ConfigRepository};
pub use domain::{Briefing, BriefingItem, Candidate, ScoreBreakdown, Source, SourceKind, Story};
pub use error::{Result, SignalError};
pub use paths::AppPaths;
pub use storage::{Store, StoreStatus};

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use chrono::{NaiveDate, TimeZone, Utc};

    use crate::{Briefing, BriefingItem, ScoreBreakdown, Story};

    pub fn story_fixture(id: &str) -> Story {
        Story {
            id: id.to_owned(),
            title: "A deterministic signal".to_owned(),
            canonical_url: format!("https://example.com/{id}"),
            excerpt: "A stable excerpt for storage tests.".to_owned(),
            category: "research".to_owned(),
            published_at: Some(Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap()),
            source_ids: vec!["example-feed".to_owned()],
            score: ScoreBreakdown {
                recency: 52.5,
                source_weight: 24.0,
                corroboration: 0.0,
                total: 76.5,
            },
            smart_summary: "A stable summary for storage tests.".to_owned(),
            is_read: false,
            is_saved: false,
        }
    }

    pub fn briefing_fixture() -> Briefing {
        Briefing {
            date: NaiveDate::from_ymd_opt(2026, 8, 29).unwrap(),
            generated_at: Utc.with_ymd_and_hms(2026, 8, 29, 9, 30, 0).unwrap(),
            items: vec![BriefingItem {
                position: 1,
                section: "top_signals".to_owned(),
                story: story_fixture("story-1"),
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn signal_error_is_public() {
        let error = crate::SignalError::InvalidConfiguration("bad source".into());
        assert_eq!(error.to_string(), "invalid configuration: bad source");
    }
}
