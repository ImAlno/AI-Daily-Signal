mod app;
mod collector;
mod config;
mod domain;
mod error;
mod paths;
mod pipeline;
mod storage;

pub use app::{RefreshReport, SignalApp};
pub use collector::{CollectionReport, FeedCollector, SourceFailure};
pub use config::{AppConfig, BriefingConfig, ConfigRepository};
pub use domain::{Briefing, BriefingItem, Candidate, ScoreBreakdown, Source, SourceKind, Story};
pub use error::{Result, SignalError};
pub use paths::AppPaths;
pub use pipeline::{
    Pipeline, PipelineOutput, assemble_briefing, deduplicate, normalize_title, normalize_url,
    score_story, smart_summary,
};
pub use storage::{RefreshRun, Store, StoreStatus};

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};

    use crate::{
        AppConfig, Briefing, BriefingConfig, BriefingItem, Candidate, ScoreBreakdown, Source,
        SourceKind, Story,
    };

    pub fn feed_source(id: &str) -> Source {
        Source {
            id: id.to_owned(),
            name: "Fixture feed".to_owned(),
            category: "research".to_owned(),
            enabled: true,
            weight: 1.0,
            kind: SourceKind::Feed {
                url: "https://example.com/feed.xml".to_owned(),
            },
        }
    }

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

    pub fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()
    }

    pub fn config_fixture() -> AppConfig {
        AppConfig {
            briefing: BriefingConfig {
                max_items: 5,
                stale_after_minutes: 60,
            },
            sources: vec![
                configured_source("primary", "Primary", "research", 0.5),
                configured_source("syndicated", "Syndicated", "research", 0.8),
                configured_source("official", "Official", "releases", 1.0),
                configured_source("low", "Low", "research", 0.1),
            ],
        }
    }

    pub fn config_with_max_items(max_items: usize) -> AppConfig {
        let mut config = config_fixture();
        config.briefing.max_items = max_items;
        config
    }

    pub fn candidate_fixture(label: &str) -> Candidate {
        Candidate {
            source_id: "primary".to_owned(),
            external_id: format!("{label}-id"),
            canonical_url: format!("https://example.com/{label}"),
            title: format!("{label} story"),
            excerpt: "A complete fixture sentence.".to_owned(),
            published_at: Some(fixed_now() - chrono::Duration::hours(1)),
            collected_at: fixed_now(),
        }
    }

    pub fn duplicate_candidates() -> Vec<Candidate> {
        let mut primary = candidate_fixture("release");
        primary.title = "Release update".to_owned();
        primary.canonical_url =
            "https://EXAMPLE.com:443/releases/1?topic=ai&utm_source=primary#details".to_owned();

        let mut syndicated = primary.clone();
        syndicated.source_id = "syndicated".to_owned();
        syndicated.external_id = "release-syndicated-id".to_owned();
        syndicated.canonical_url =
            "https://example.com/releases/1?fbclid=tracker&topic=ai".to_owned();
        syndicated.excerpt =
            "A longer complete fixture sentence with corroborating detail.".to_owned();

        vec![primary, syndicated]
    }

    pub fn ranked_candidates() -> Vec<Candidate> {
        let mut official = candidate_fixture("official-release");
        official.source_id = "official".to_owned();
        official.title = "New official release".to_owned();
        official.published_at = Some(fixed_now() - chrono::Duration::hours(1));

        let mut low = candidate_fixture("old-low-weight");
        low.source_id = "low".to_owned();
        low.title = "Old low weight story".to_owned();
        low.published_at = Some(fixed_now() - chrono::Duration::days(5));

        vec![low, official]
    }

    fn configured_source(id: &str, name: &str, category: &str, weight: f64) -> Source {
        Source {
            id: id.to_owned(),
            name: name.to_owned(),
            category: category.to_owned(),
            enabled: true,
            weight,
            kind: SourceKind::Feed {
                url: format!("https://example.com/{id}.xml"),
            },
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
