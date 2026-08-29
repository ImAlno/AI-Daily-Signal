mod app;
mod collector;
mod config;
mod credentials;
mod domain;
mod error;
mod generator;
mod models;
mod paths;
mod pipeline;
mod providers;
mod storage;
mod summaries;

pub use app::{
    AddModelCredential, AddModelInput, AddModelReport, CredentialWarningKind, RefreshOptions,
    RefreshReport, RemoveModelReport, SignalApp, TodayView,
};
pub use collector::{CollectionReport, FeedCollector, SourceFailure};
pub use config::{AppConfig, BriefingConfig, ConfigRepository};
pub use credentials::{
    CredentialResolver, CredentialStore, EnvironmentReader, ProcessEnvironmentReader,
    ResolvedCredential, SystemCredentialStore, persist_system_credential_then,
};
pub use domain::{Briefing, BriefingItem, Candidate, ScoreBreakdown, Source, SourceKind, Story};
pub use error::{Result, SignalError};
pub use generator::{
    AiGenerationCoordinator, ManualGenerationStatus, SummarizeOptions, SummarizeReport,
    TestModelOptions, TestModelReport,
};
pub use models::{
    ApiDialect, CredentialRef, ModelProfile, MoneyMicros, NewModelProfile, ProfileLimits,
    ProviderKind,
};
pub use paths::AppPaths;
pub use pipeline::{
    Pipeline, PipelineOutput, assemble_briefing, deduplicate, normalize_title, normalize_url,
    score_story, smart_summary,
};
pub use providers::{
    AI_SUMMARY_PROMPT_VERSION, AiSummaryPrompt, AnthropicProvider, GeminiProvider, OpenAiProvider,
    ProviderFailure, ProviderFailureKind, ProviderRegistry, ProviderRequest, ProviderResponse,
    ProviderUsage, RequestChargeStatus, RetryAttemptFailure, RetryPolicy, RetrySleeper,
    SummaryProvider, TokioRetrySleeper, build_ai_summary_prompt, parse_ai_summary,
    retry_provider_operation,
};
pub use storage::{RefreshRun, Store, StoreStatus};
pub use summaries::{
    AiSummaryFields, AttemptOutcome, BudgetDecision, BudgetReservation, GenerationAttempt,
    GenerationFailureKind, GenerationOutcomeKind, GenerationReport, GenerationStatus,
    SummarySettings, SummaryVariant, summary_cache_key,
};

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use std::{
        collections::HashMap,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        thread::{self, JoinHandle},
        time::Duration as StdDuration,
    };

    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    use secrecy::SecretString;
    use sha2::Digest;

    use crate::{
        AiSummaryFields, ApiDialect, AppConfig, AppPaths, Briefing, BriefingConfig, BriefingItem,
        Candidate, ConfigRepository, CredentialRef, CredentialStore, EnvironmentReader,
        FeedCollector, ModelProfile, Pipeline, ProfileLimits, ProviderFailure, ProviderFailureKind,
        ProviderKind, ProviderRegistry, ProviderRequest, ProviderResponse, ProviderUsage,
        RequestChargeStatus, ScoreBreakdown, SignalApp, Source, SourceKind, Store, Story,
        SummaryProvider, SummarySettings, SummaryVariant,
    };

    pub use crate::credentials::MemoryCredentialStore;

    pub fn provider_http_client() -> Result<reqwest::Client, crate::ProviderFailure> {
        crate::providers::shared_http_client()
    }

    pub async fn read_provider_json(
        response: reqwest::Response,
    ) -> Result<serde_json::Value, crate::ProviderFailure> {
        crate::providers::read_json_response(response).await
    }

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

    const AI_FIXTURE_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>AI fixture</title>
  <item><guid>fixture-first</guid><title>Highest ranked public signal</title>
    <link>https://example.com/fixture-first</link>
    <description>A complete first fixture sentence with public information.</description>
    <pubDate>Sat, 29 Aug 2026 11:30:00 +0000</pubDate></item>
  <item><guid>fixture-second</guid><title>Second ranked public signal</title>
    <link>https://example.com/fixture-second</link>
    <description>A complete second fixture sentence with public information.</description>
    <pubDate>Sat, 29 Aug 2026 10:30:00 +0000</pubDate></item>
  <item><guid>fixture-third</guid><title>Third ranked public signal</title>
    <link>https://example.com/fixture-third</link>
    <description>A complete third fixture sentence with public information.</description>
    <pubDate>Sat, 29 Aug 2026 09:30:00 +0000</pubDate></item>
</channel></rss>"#;

    #[derive(Default)]
    pub struct MemoryEnvironmentReader {
        values: Mutex<HashMap<String, Option<String>>>,
    }

    impl MemoryEnvironmentReader {
        pub fn set(&self, variable: &str, value: Option<String>) {
            self.values
                .lock()
                .expect("memory environment mutex")
                .insert(variable.to_owned(), value);
        }
    }

    impl EnvironmentReader for MemoryEnvironmentReader {
        fn read(&self, variable: &str) -> crate::Result<Option<String>> {
            Ok(self
                .values
                .lock()
                .expect("memory environment mutex")
                .get(variable)
                .cloned()
                .flatten())
        }
    }

    #[derive(Clone, Copy)]
    enum RecordingProviderMode {
        Success(Option<ProviderUsage>),
        Failure(ProviderFailure),
        Malformed,
    }

    pub struct RecordingProvider {
        requested_story_ids: Mutex<Vec<String>>,
        mode: Mutex<RecordingProviderMode>,
    }

    impl Default for RecordingProvider {
        fn default() -> Self {
            Self {
                requested_story_ids: Mutex::new(Vec::new()),
                mode: Mutex::new(RecordingProviderMode::Success(Some(ProviderUsage {
                    input_tokens: 120,
                    output_tokens: 60,
                }))),
            }
        }
    }

    impl RecordingProvider {
        pub fn request_count(&self) -> usize {
            self.requested_story_ids
                .lock()
                .expect("recording provider mutex")
                .len()
        }

        pub fn requested_story_ids(&self) -> Vec<String> {
            self.requested_story_ids
                .lock()
                .expect("recording provider mutex")
                .clone()
        }

        fn fail_with(&self, kind: ProviderFailureKind, charge_status: RequestChargeStatus) {
            *self.mode.lock().expect("recording provider mutex") =
                RecordingProviderMode::Failure(ProviderFailure::new(kind, charge_status));
        }

        fn return_malformed(&self) {
            *self.mode.lock().expect("recording provider mutex") = RecordingProviderMode::Malformed;
        }

        fn report_usage(&self, usage: Option<ProviderUsage>) {
            *self.mode.lock().expect("recording provider mutex") =
                RecordingProviderMode::Success(usage);
        }
    }

    #[async_trait::async_trait]
    impl SummaryProvider for RecordingProvider {
        async fn generate(
            &self,
            request: &ProviderRequest,
            _credential: &crate::ResolvedCredential,
        ) -> std::result::Result<ProviderResponse, ProviderFailure> {
            self.requested_story_ids
                .lock()
                .expect("recording provider mutex")
                .push(request.story_id.clone());
            match *self.mode.lock().expect("recording provider mutex") {
                RecordingProviderMode::Success(usage) => Ok(ProviderResponse {
                    fields: AiSummaryFields {
                        what_happened: "A deterministic public fixture event happened.".to_owned(),
                        why_it_matters: "It demonstrates the coordinator contract.".to_owned(),
                        caveat: Some("The story is synthetic.".to_owned()),
                    },
                    usage,
                }),
                RecordingProviderMode::Failure(failure) => Err(failure),
                RecordingProviderMode::Malformed => Ok(ProviderResponse {
                    fields: AiSummaryFields {
                        what_happened: "".to_owned(),
                        why_it_matters: "Invalid blank required field.".to_owned(),
                        caveat: None,
                    },
                    usage: Some(ProviderUsage {
                        input_tokens: 1,
                        output_tokens: 1,
                    }),
                }),
            }
        }
    }

    pub struct AiAppFixture {
        pub app: SignalApp,
        pub now: DateTime<Utc>,
        pub provider: Arc<RecordingProvider>,
        pub credential_store: Arc<MemoryCredentialStore>,
        pub environment_reader: Arc<MemoryEnvironmentReader>,
        paths: AppPaths,
        provider_registry: Arc<ProviderRegistry>,
        profile_id: uuid::Uuid,
        _root: TemporaryRoot,
        feed_server: FixtureFeedServer,
    }

    impl AiAppFixture {
        pub fn with_max_items(mut self, maximum: usize) -> Self {
            let mut config = ConfigRepository::new(self.paths.clone())
                .load_or_create()
                .expect("fixture config");
            config.briefing.max_items = maximum;
            ConfigRepository::new(self.paths.clone())
                .save(&config)
                .expect("fixture config save");
            self.reopen();
            self
        }

        pub fn with_refresh_cap(self, maximum: u32) -> Self {
            self.update_profile(|profile| {
                profile.limits.max_summaries_per_refresh = maximum;
            })
        }

        pub fn with_cached_story_at(self, index: usize) -> Self {
            let story = self.selected_story(index);
            let profile = self.profile();
            let settings = SummarySettings::default();
            let cache_key = crate::summary_cache_key(
                &story,
                &profile,
                crate::AI_SUMMARY_PROMPT_VERSION,
                &settings,
            )
            .expect("fixture cache key");
            let store = self.store();
            store
                .upsert_stories(std::slice::from_ref(&story))
                .expect("fixture story insert");
            let mut variant = summary_variant(
                "ai-app-cached-variant",
                &cache_key,
                self.now - chrono::Duration::seconds(1),
            );
            variant.story_id = story.id;
            variant.profile_id = Some(profile.id);
            variant.provider = profile.provider;
            variant.model = profile.model;
            store
                .insert_summary_variant(&variant)
                .expect("fixture cached variant");
            self
        }

        pub fn with_provider_failure(self, kind: ProviderFailureKind) -> Self {
            self.provider
                .fail_with(kind, RequestChargeStatus::PossiblySent);
            self
        }

        pub fn with_provider_failure_status(
            self,
            kind: ProviderFailureKind,
            charge_status: RequestChargeStatus,
        ) -> Self {
            self.provider.fail_with(kind, charge_status);
            self
        }

        pub fn with_budget_for_one_request(self) -> Self {
            self.provider.report_usage(None);
            self.update_profile(|profile| {
                profile.limits.max_daily_cost_microusd = Some(1_000);
            })
        }

        pub fn without_default_profile(mut self) -> Self {
            self.store()
                .set_default_model_profile(None)
                .expect("fixture default removal");
            self.reopen();
            self
        }

        pub fn without_consent(self) -> Self {
            self.update_profile(|profile| profile.consented_at = None)
        }

        pub fn without_credential(self) -> Self {
            let profile = self.profile();
            self.credential_store
                .delete(&profile.credential)
                .expect("fixture credential deletion");
            self
        }

        pub fn with_empty_environment_credential(self) -> Self {
            self.environment_reader
                .set("EMPTY_FIXTURE_KEY", Some(String::new()));
            self.update_profile(|profile| {
                profile.credential = CredentialRef::Environment {
                    variable: "EMPTY_FIXTURE_KEY".to_owned(),
                };
            })
        }

        pub fn with_malformed_output(self) -> Self {
            self.provider.return_malformed();
            self
        }

        pub fn with_provider_invalid_gemini_model(self) -> Self {
            self.update_profile(|profile| {
                profile.provider = ProviderKind::Gemini;
                profile.model = "invalid\nmodel".to_owned();
            })
        }

        pub fn with_cost_overflow(self) -> Self {
            self.update_profile(|profile| {
                profile.limits.max_output_tokens = u32::MAX;
                profile.limits.input_cost_microusd_per_million = Some(i64::MAX as u64);
                profile.limits.output_cost_microusd_per_million = Some(i64::MAX as u64);
            })
        }

        pub fn with_sqlite_cost_range_overflow(self) -> Self {
            self.update_profile(|profile| {
                profile.limits.max_output_tokens = u32::MAX;
                profile.limits.input_cost_microusd_per_million = Some(3_000_000_000_000_000);
                profile.limits.output_cost_microusd_per_million = Some(3_000_000_000_000_000);
            })
        }

        pub fn with_unreported_usage(self) -> Self {
            self.provider.report_usage(None);
            self
        }

        pub fn with_unpersistable_reported_usage(self) -> Self {
            self.provider.report_usage(Some(ProviderUsage {
                input_tokens: u64::MAX,
                output_tokens: 0,
            }));
            self
        }

        pub fn with_unpriced_profile(self) -> Self {
            self.update_profile(|profile| {
                profile.limits.input_cost_microusd_per_million = None;
                profile.limits.output_cost_microusd_per_million = None;
            })
        }

        pub fn profile(&self) -> ModelProfile {
            self.store()
                .find_model_profile(self.profile_id)
                .expect("fixture profile lookup")
                .expect("fixture profile")
        }

        pub fn store(&self) -> Store {
            Store::open(self.paths.data_dir.join("signal.sqlite3"))
                .expect("fixture store should open")
        }

        pub fn feed_server_stats(&self) -> (usize, usize, usize) {
            self.feed_server.stats()
        }

        fn selected_story(&self, index: usize) -> Story {
            let config = ConfigRepository::new(self.paths.clone())
                .load_or_create()
                .expect("fixture config");
            let candidates =
                FeedCollector::parse(&config.sources[0], AI_FIXTURE_FEED.as_bytes(), self.now)
                    .expect("fixture feed parse");
            Pipeline::build(candidates, &config, self.now)
                .briefing
                .items[index]
                .story
                .clone()
        }

        fn update_profile(mut self, change: impl FnOnce(&mut ModelProfile)) -> Self {
            let store = self.store();
            let mut profile = self.profile();
            change(&mut profile);
            profile.updated_at = self.now;
            store
                .remove_model_profile(profile.id)
                .expect("fixture profile removal");
            store
                .create_model_profile(&profile)
                .expect("fixture profile replacement");
            store
                .set_default_model_profile(Some(profile.id))
                .expect("fixture default profile");
            self.reopen();
            self
        }

        fn reopen(&mut self) {
            self.app = SignalApp::open_with_services(
                self.paths.clone(),
                self.credential_store.clone(),
                self.environment_reader.clone(),
                self.provider_registry.clone(),
            )
            .expect("fixture app should reopen");
        }
    }

    pub fn ai_app_fixture() -> AiAppFixture {
        let root_path = std::env::temp_dir().join(format!(
            "signal-core-ai-app-{}",
            uuid::Uuid::new_v4().hyphenated()
        ));
        std::fs::create_dir_all(&root_path).expect("fixture root");
        let root = TemporaryRoot(root_path.clone());
        let paths = AppPaths::for_root(&root_path);
        let feed_server = FixtureFeedServer::start(AI_FIXTURE_FEED);
        let config = AppConfig {
            briefing: BriefingConfig {
                max_items: 3,
                stale_after_minutes: 60,
            },
            sources: vec![Source {
                id: "ai-fixture".to_owned(),
                name: "AI fixture feed".to_owned(),
                category: "research".to_owned(),
                enabled: true,
                weight: 1.0,
                kind: SourceKind::Feed {
                    url: feed_server.url(),
                },
            }],
        };
        ConfigRepository::new(paths.clone())
            .save(&config)
            .expect("fixture config save");

        let store =
            Store::open(paths.data_dir.join("signal.sqlite3")).expect("fixture store should open");
        let mut profile = model_profile("fixture", ProviderKind::OpenAi);
        profile.limits.input_cost_microusd_per_million = Some(1_000_000);
        profile.limits.output_cost_microusd_per_million = Some(1_000_000);
        store
            .create_model_profile(&profile)
            .expect("fixture profile insert");
        store
            .set_default_model_profile(Some(profile.id))
            .expect("fixture default profile");

        let credential_store = Arc::new(MemoryCredentialStore::default());
        credential_store
            .set(
                &profile.credential,
                SecretString::from("fixture-secret".to_owned()),
            )
            .expect("fixture credential");
        let environment_reader = Arc::new(MemoryEnvironmentReader::default());
        let provider = Arc::new(RecordingProvider::default());
        let mut registry = ProviderRegistry::new();
        registry.register(ProviderKind::OpenAi, provider.clone());
        let provider_registry = Arc::new(registry);
        let app = SignalApp::open_with_services(
            paths.clone(),
            credential_store.clone(),
            environment_reader.clone(),
            provider_registry.clone(),
        )
        .expect("fixture app should open");

        AiAppFixture {
            app,
            now: fixed_now(),
            provider,
            credential_store,
            environment_reader,
            paths,
            provider_registry,
            profile_id: profile.id,
            _root: root,
            feed_server,
        }
    }

    struct TemporaryRoot(PathBuf);

    impl Drop for TemporaryRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct FixtureFeedServer {
        address: std::net::SocketAddr,
        shutdown: Arc<AtomicBool>,
        accepted: Arc<AtomicUsize>,
        responses_written: Arc<AtomicUsize>,
        accept_errors: Arc<AtomicUsize>,
        thread: Option<JoinHandle<()>>,
    }

    impl FixtureFeedServer {
        fn start(body: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("fixture feed bind");
            let address = listener.local_addr().expect("fixture feed address");
            let shutdown = Arc::new(AtomicBool::new(false));
            let worker_shutdown = shutdown.clone();
            let accepted = Arc::new(AtomicUsize::new(0));
            let responses_written = Arc::new(AtomicUsize::new(0));
            let accept_errors = Arc::new(AtomicUsize::new(0));
            let worker_accepted = accepted.clone();
            let worker_responses_written = responses_written.clone();
            let worker_accept_errors = accept_errors.clone();
            let thread = thread::spawn(move || {
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if worker_shutdown.load(Ordering::SeqCst) {
                                return;
                            }
                            worker_accepted.fetch_add(1, Ordering::SeqCst);
                            if serve_fixture_feed(stream, body) {
                                worker_responses_written.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => {
                            worker_accept_errors.fetch_add(1, Ordering::SeqCst);
                            return;
                        }
                    }
                }
            });
            Self {
                address,
                shutdown,
                accepted,
                responses_written,
                accept_errors,
                thread: Some(thread),
            }
        }

        fn url(&self) -> String {
            format!("http://{}/feed.xml", self.address)
        }

        fn stats(&self) -> (usize, usize, usize) {
            (
                self.accepted.load(Ordering::SeqCst),
                self.responses_written.load(Ordering::SeqCst),
                self.accept_errors.load(Ordering::SeqCst),
            )
        }
    }

    impl Drop for FixtureFeedServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::SeqCst);
            let _ = TcpStream::connect(self.address);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn serve_fixture_feed(mut stream: TcpStream, body: &str) -> bool {
        let _ = stream.set_read_timeout(Some(StdDuration::from_secs(1)));
        let mut request = [0_u8; 8 * 1024];
        let mut received = 0;
        while received < request.len() {
            match stream.read(&mut request[received..]) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    received += count;
                    if request[..received]
                        .windows(4)
                        .any(|window| window == b"\r\n\r\n")
                    {
                        break;
                    }
                }
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/rss+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).is_ok() && stream.flush().is_ok()
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
