mod app;
mod collector;
mod config;
mod credentials;
mod domain;
mod error;
mod models;
mod paths;
mod pipeline;
mod storage;
mod summaries;

pub use app::{RefreshReport, SignalApp, TodayView};
pub use collector::{CollectionReport, FeedCollector, SourceFailure};
pub use config::{AppConfig, BriefingConfig, ConfigRepository};
pub use credentials::{
    CredentialResolver, CredentialStore, EnvironmentReader, ProcessEnvironmentReader,
    ResolvedCredential, SystemCredentialStore, persist_system_credential_then,
};
pub use domain::{Briefing, BriefingItem, Candidate, ScoreBreakdown, Source, SourceKind, Story};
pub use error::{Result, SignalError};
pub use models::{
    ApiDialect, CredentialRef, ModelProfile, MoneyMicros, NewModelProfile, ProfileLimits,
    ProviderKind,
};
pub use paths::AppPaths;
pub use pipeline::{
    Pipeline, PipelineOutput, assemble_briefing, deduplicate, normalize_title, normalize_url,
    score_story, smart_summary,
};
pub use storage::{RefreshRun, Store, StoreStatus};
pub use summaries::{
    AiSummaryFields, AttemptOutcome, BudgetDecision, BudgetReservation, GenerationAttempt,
    GenerationFailureKind, GenerationReport, GenerationStatus, SummarySettings, SummaryVariant,
    summary_cache_key,
};

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use std::path::PathBuf;

    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    use sha2::Digest;

    use crate::{
        AiSummaryFields, ApiDialect, AppConfig, Briefing, BriefingConfig, BriefingItem, Candidate,
        CredentialRef, ModelProfile, ProfileLimits, ProviderKind, ScoreBreakdown, Source,
        SourceKind, Store, Story, SummarySettings, SummaryVariant,
    };

    pub use crate::credentials::MemoryCredentialStore;

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
                is_stale: false,
                story: story_fixture("story-1"),
                selected_summary: None,
            }],
        }
    }

    pub fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()
    }

    pub fn temporary_store() -> Store {
        let path =
            std::env::temp_dir().join(format!("signal-core-{}.sqlite3", uuid::Uuid::new_v4()));
        let store = Store::open(path).unwrap();
        store.upsert_stories(&[story_fixture("story-1")]).unwrap();
        store
    }

    pub fn model_profile(name: &str, provider: ProviderKind) -> ModelProfile {
        let mut digest = sha2::Sha256::digest(name.as_bytes());
        digest[6] = (digest[6] & 0x0f) | 0x50;
        digest[8] = (digest[8] & 0x3f) | 0x80;
        let id = uuid::Uuid::from_slice(&digest[..16]).unwrap();
        ModelProfile {
            id,
            name: name.to_owned(),
            provider,
            model: format!("{name}-model"),
            endpoint: None,
            dialect: None,
            credential: CredentialRef::for_profile(id),
            consented_at: Some(fixed_now()),
            enabled: true,
            limits: ProfileLimits::default(),
            created_at: fixed_now(),
            updated_at: fixed_now(),
        }
    }

    pub fn summary_variant(
        id_seed: &str,
        cache_key: &str,
        generated_at: DateTime<Utc>,
    ) -> SummaryVariant {
        let digest = sha2::Sha256::digest(id_seed.as_bytes());
        SummaryVariant {
            id: uuid::Uuid::from_slice(&digest[..16]).unwrap(),
            story_id: "story-1".to_owned(),
            profile_id: None,
            provider: ProviderKind::OpenAi,
            model: "fixture-model".to_owned(),
            endpoint: None,
            dialect: None,
            prompt_version: "ai-summary-v1".to_owned(),
            cache_key: cache_key.to_owned(),
            fields: AiSummaryFields {
                what_happened: "A deterministic event happened.".to_owned(),
                why_it_matters: "It has a deterministic consequence.".to_owned(),
                caveat: Some("The fixture is synthetic.".to_owned()),
            },
            input_tokens: Some(120),
            output_tokens: Some(60),
            cost_microusd: 42,
            generated_at,
        }
    }

    #[derive(Clone)]
    pub struct CacheIdentityFixture {
        pub story: Story,
        pub profile: ModelProfile,
        pub prompt_version: String,
        pub settings: SummarySettings,
    }

    impl CacheIdentityFixture {
        pub fn each_single_field_changed(&self) -> Vec<Self> {
            let mut values = Vec::new();
            macro_rules! changed {
                ($body:expr) => {{
                    let mut fixture = self.clone();
                    $body(&mut fixture);
                    values.push(fixture);
                }};
            }
            changed!(|fixture: &mut Self| fixture.story.title = "A changed title".to_owned());
            changed!(|fixture: &mut Self| fixture.story.excerpt = "Changed excerpt".to_owned());
            changed!(|fixture: &mut Self| fixture.story.canonical_url =
                "https://example.com/changed".to_owned());
            changed!(|fixture: &mut Self| fixture.story.published_at =
                Some(fixed_now() + chrono::Duration::seconds(1)));
            changed!(|fixture: &mut Self| fixture.story.category = "releases".to_owned());
            changed!(|fixture: &mut Self| fixture.story.source_ids =
                vec!["example-feed".to_owned(), "second-feed".to_owned()]);
            changed!(|fixture: &mut Self| fixture.profile.provider = ProviderKind::Anthropic);
            changed!(|fixture: &mut Self| fixture.profile.endpoint =
                Some("https://other.example/v1".parse().unwrap()));
            changed!(|fixture: &mut Self| fixture.profile.model = "opaque/model:changed".to_owned());
            changed!(
                |fixture: &mut Self| fixture.profile.dialect = Some(ApiDialect::ChatCompletions)
            );
            changed!(|fixture: &mut Self| fixture.prompt_version = "ai-summary-v2".to_owned());
            changed!(|fixture: &mut Self| fixture.profile.limits.max_output_tokens += 1);
            changed!(|fixture: &mut Self| fixture.settings.what_happened_max_chars += 1);
            changed!(|fixture: &mut Self| fixture.settings.why_it_matters_max_chars += 1);
            changed!(|fixture: &mut Self| fixture.settings.caveat_max_chars += 1);
            values
        }
    }

    pub fn cache_identity_fixture() -> CacheIdentityFixture {
        let mut profile = model_profile("cache", ProviderKind::OpenAiCompatible);
        profile.endpoint = Some("https://provider.example/v1/".parse().unwrap());
        profile.dialect = Some(ApiDialect::Responses);
        profile.credential = CredentialRef::Environment {
            variable: "CACHE_API_KEY".to_owned(),
        };
        CacheIdentityFixture {
            story: story_fixture("story-1"),
            profile,
            prompt_version: "ai-summary-v1".to_owned(),
            settings: SummarySettings::default(),
        }
    }

    pub struct SharedBudgetStore {
        pub store: Store,
        pub profile: ModelProfile,
        pub now: DateTime<Utc>,
        pub expires_at: DateTime<Utc>,
    }

    pub fn shared_budget_store(daily_limit_microusd: u64) -> SharedBudgetStore {
        let path = std::env::temp_dir().join(format!(
            "signal-core-budget-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let store = Store::open(path).unwrap();
        let mut profile = model_profile("budget", ProviderKind::OpenAi);
        profile.limits.max_daily_cost_microusd = Some(daily_limit_microusd);
        profile.limits.input_cost_microusd_per_million = Some(1);
        profile.limits.output_cost_microusd_per_million = Some(1);
        store.create_model_profile(&profile).unwrap();
        let now = fixed_now();
        SharedBudgetStore {
            store,
            profile,
            now,
            expires_at: now + chrono::Duration::minutes(10),
        }
    }

    pub struct VersionTwoDatabase {
        pub path: PathBuf,
    }

    pub fn version_two_database() -> VersionTwoDatabase {
        let path =
            std::env::temp_dir().join(format!("signal-core-v2-{}.sqlite3", uuid::Uuid::new_v4()));
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!(
                "../migrations/002_briefing_item_staleness.sql"
            ))
            .unwrap();
        connection
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (1, ?1), (2, ?1)",
                [fixed_now().to_rfc3339()],
            )
            .unwrap();
        let story = story_fixture("story-1");
        connection
            .execute(
                "INSERT INTO stories (
                     id, title, canonical_url, excerpt, category, published_at, source_ids_json,
                     score_json, smart_summary, is_read, is_saved, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 1, ?10)",
                rusqlite::params![
                    story.id,
                    story.title,
                    story.canonical_url,
                    story.excerpt,
                    story.category,
                    story.published_at.map(|value| value.to_rfc3339()),
                    serde_json::to_string(&story.source_ids).unwrap(),
                    serde_json::to_string(&story.score).unwrap(),
                    story.smart_summary,
                    fixed_now().to_rfc3339(),
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO briefings (date, generated_at) VALUES (?1, ?2)",
                rusqlite::params![
                    fixed_now().date_naive().to_string(),
                    fixed_now().to_rfc3339()
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO briefing_items (
                     briefing_date, story_id, position, section, is_stale
                 ) VALUES (?1, 'story-1', 1, 'top_signals', 0)",
                [fixed_now().date_naive().to_string()],
            )
            .unwrap();
        drop(connection);
        VersionTwoDatabase { path }
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
