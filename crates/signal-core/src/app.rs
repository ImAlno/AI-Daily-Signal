use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use secrecy::SecretString;
use url::Url;
use uuid::Uuid;

use crate::credentials::nonempty_secret;
use crate::{
    AiGenerationCoordinator, AnthropicProvider, ApiDialect, AppConfig, AppPaths, Briefing,
    ConfigRepository, CredentialRef, CredentialStore, EnvironmentReader, FeedCollector,
    GeminiProvider, GenerationReport, ModelProfile, NewModelProfile, OpenAiProvider, Pipeline,
    ProcessEnvironmentReader, ProfileLimits, ProviderKind, ProviderRegistry, Result, SignalError,
    Source, SourceFailure, Store, StoreStatus, Story, SummarizeOptions, SummarizeReport,
    SystemCredentialStore, TestModelOptions, TestModelReport, persist_system_credential_then,
};

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RefreshOptions {
    pub ai: bool,
}

impl Default for RefreshOptions {
    fn default() -> Self {
        Self { ai: true }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RefreshReport {
    pub briefing: Briefing,
    pub successful_sources: usize,
    pub failures: Vec<SourceFailure>,
    #[serde(default)]
    pub generation: GenerationReport,
}

pub enum AddModelCredential {
    SystemStore { secret: SecretString },
    Environment { variable: String },
}

pub struct AddModelInput {
    pub name: String,
    pub provider: ProviderKind,
    pub model: String,
    pub endpoint: Option<Url>,
    pub dialect: Option<ApiDialect>,
    pub credential: AddModelCredential,
    pub consented_at: Option<DateTime<Utc>>,
    pub enabled: bool,
    pub limits: ProfileLimits,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AddModelReport {
    pub profile: ModelProfile,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialWarningKind {
    DeleteFailed,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RemoveModelReport {
    pub removed_profile_id: Uuid,
    pub credential_deleted: bool,
    pub warning: Option<CredentialWarningKind>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TodayView {
    #[serde(flatten)]
    pub briefing: Briefing,
    pub is_stale: bool,
}

impl TodayView {
    pub fn fresh(briefing: Briefing) -> Self {
        Self {
            briefing,
            is_stale: false,
        }
    }
}

pub struct SignalApp {
    paths: AppPaths,
    config: AppConfig,
    store: Store,
    credential_store: Arc<dyn CredentialStore>,
    environment_reader: Arc<dyn EnvironmentReader>,
    provider_registry: Arc<ProviderRegistry>,
}

impl SignalApp {
    pub fn open() -> Result<Self> {
        let paths = match std::env::var_os("SIGNAL_HOME") {
            Some(root) if !root.is_empty() => AppPaths::for_root(&PathBuf::from(root)),
            Some(_) => {
                return Err(SignalError::InvalidConfiguration(
                    "SIGNAL_HOME cannot be empty".to_owned(),
                ));
            }
            None => AppPaths::discover().ok_or_else(|| {
                SignalError::InvalidConfiguration(
                    "application directories are unavailable".to_owned(),
                )
            })?,
        };
        let mut registry = ProviderRegistry::new();
        registry.register(ProviderKind::OpenAi, Arc::new(OpenAiProvider::official()?));
        registry.register(
            ProviderKind::OpenAiCompatible,
            Arc::new(OpenAiProvider::compatible()?),
        );
        registry.register(
            ProviderKind::Anthropic,
            Arc::new(AnthropicProvider::official()?),
        );
        registry.register(ProviderKind::Gemini, Arc::new(GeminiProvider::official()?));
        Self::open_at_paths(
            paths,
            Arc::new(SystemCredentialStore),
            Arc::new(ProcessEnvironmentReader),
            Arc::new(registry),
        )
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn open_with_services(
        paths: AppPaths,
        credential_store: Arc<dyn CredentialStore>,
        environment_reader: Arc<dyn EnvironmentReader>,
        provider_registry: Arc<ProviderRegistry>,
    ) -> Result<Self> {
        Self::open_at_paths(
            paths,
            credential_store,
            environment_reader,
            provider_registry,
        )
    }

    fn open_at_paths(
        paths: AppPaths,
        credential_store: Arc<dyn CredentialStore>,
        environment_reader: Arc<dyn EnvironmentReader>,
        provider_registry: Arc<ProviderRegistry>,
    ) -> Result<Self> {
        let config = ConfigRepository::new(paths.clone()).load_or_create()?;
        let store = storage_result(Store::open(paths.data_dir.join("signal.sqlite3")))?;
        Ok(Self {
            paths,
            config,
            store,
            credential_store,
            environment_reader,
            provider_registry,
        })
    }

    pub fn init(&self) -> Result<StoreStatus> {
        storage_result(self.store.status())
    }

    pub async fn refresh(&self, now: DateTime<Utc>) -> Result<RefreshReport> {
        self.refresh_with_options(now, RefreshOptions::default())
            .await
    }

    pub async fn refresh_with_options(
        &self,
        now: DateTime<Utc>,
        options: RefreshOptions,
    ) -> Result<RefreshReport> {
        if !self.config.sources.iter().any(|source| source.enabled) {
            return Err(SignalError::InvalidConfiguration(
                "at least one source must be enabled".to_owned(),
            ));
        }

        let collection = FeedCollector::new()?
            .collect_all(&self.config.sources, now)
            .await;
        let successful_sources = collection.successful_source_ids.len();
        let failed_sources = collection.failures.len();
        if collection.successful_source_ids.is_empty() {
            storage_result(self.store.record_refresh_failure(now, failed_sources))?;
            return Err(SignalError::Refresh(
                "every enabled source failed".to_owned(),
            ));
        }

        let failures = collection.failures;
        let previous = if failures.is_empty() {
            None
        } else {
            storage_result(self.store.load_latest_briefing())?
        };
        let mut output = Pipeline::build(collection.candidates, &self.config, now);
        merge_partial_briefing(
            &mut output.briefing,
            previous.as_ref(),
            &failures,
            self.config.briefing.max_items,
        );
        for item in &mut output.briefing.items {
            item.selected_summary = None;
        }
        let generation = if options.ai {
            storage_result(self.store.upsert_stories(&output.stories))?;
            let profile = storage_result(self.store.default_model_profile())?;
            self.coordinator()
                .generate_briefing(&mut output.briefing, profile.as_ref(), now)
                .await?
        } else {
            GenerationReport::default()
        };
        storage_result(self.store.commit_refresh_with_counts(
            &output.stories,
            &output.briefing,
            successful_sources,
            failed_sources,
        ))?;

        Ok(RefreshReport {
            briefing: output.briefing,
            successful_sources,
            failures,
            generation,
        })
    }

    pub fn list_models(&self) -> Result<Vec<ModelProfile>> {
        storage_result(self.store.list_model_profiles())
    }

    pub fn default_model_profile(&self) -> Result<Option<ModelProfile>> {
        storage_result(self.store.default_model_profile())
    }

    pub fn add_model(&self, input: AddModelInput, now: DateTime<Utc>) -> Result<AddModelReport> {
        if input.consented_at.is_none() {
            return Err(SignalError::InvalidConfiguration(
                "model provider data sharing consent is required".to_owned(),
            ));
        }
        let id = Uuid::new_v4();
        let (credential, system_secret) = match input.credential {
            AddModelCredential::SystemStore { secret } => (
                CredentialRef::for_profile(id),
                Some(nonempty_secret(secret)?),
            ),
            AddModelCredential::Environment { variable } => {
                (CredentialRef::Environment { variable }, None)
            }
        };
        let profile = NewModelProfile {
            name: input.name,
            provider: input.provider,
            model: input.model,
            endpoint: input.endpoint,
            dialect: input.dialect,
            credential,
            consented_at: input.consented_at,
            enabled: input.enabled,
            limits: input.limits,
        }
        .into_model_profile(id, now, now)?;
        match system_secret {
            Some(secret) => persist_system_credential_then(
                self.credential_store.as_ref(),
                &profile.credential,
                secret,
                || storage_result(self.store.create_model_profile(&profile)),
            )?,
            None => storage_result(self.store.create_model_profile(&profile))?,
        }
        Ok(AddModelReport { profile })
    }

    pub fn use_model(&self, profile: &str) -> Result<ModelProfile> {
        let profile = self.find_model_selector(profile)?;
        if !profile.enabled || profile.consented_at.is_none() {
            return Err(SignalError::InvalidConfiguration(
                "default model profile must be enabled and consented".to_owned(),
            ));
        }
        storage_result(self.store.set_default_model_profile(Some(profile.id)))?;
        Ok(profile)
    }

    pub async fn test_model(
        &self,
        options: TestModelOptions,
        now: DateTime<Utc>,
    ) -> Result<TestModelReport> {
        let profile = self.find_model_selector(&options.profile)?;
        self.coordinator().test_model(&profile, now).await
    }

    pub fn remove_model(&self, profile: &str) -> Result<RemoveModelReport> {
        let profile = self.find_model_selector(profile)?;
        storage_result(self.store.remove_model_profile(profile.id))?;
        match &profile.credential {
            CredentialRef::Environment { .. } => Ok(RemoveModelReport {
                removed_profile_id: profile.id,
                credential_deleted: false,
                warning: None,
            }),
            CredentialRef::SystemStore { .. } => {
                match self.credential_store.delete(&profile.credential) {
                    Ok(()) => Ok(RemoveModelReport {
                        removed_profile_id: profile.id,
                        credential_deleted: true,
                        warning: None,
                    }),
                    Err(_) => Ok(RemoveModelReport {
                        removed_profile_id: profile.id,
                        credential_deleted: false,
                        warning: Some(CredentialWarningKind::DeleteFailed),
                    }),
                }
            }
        }
    }

    pub async fn summarize_story(
        &self,
        story_id: &str,
        options: SummarizeOptions,
        now: DateTime<Utc>,
    ) -> Result<SummarizeReport> {
        let story = self.show(story_id)?;
        let profile = match options.profile {
            Some(selector) => self.find_model_selector(&selector)?,
            None => storage_result(self.store.default_model_profile())?.ok_or_else(|| {
                SignalError::NotFound("No default model profile is configured".to_owned())
            })?,
        };
        let report = self
            .coordinator()
            .summarize(&story, &profile, options.force, now)
            .await?;
        if let Some(summary) = &report.summary {
            storage_result(
                self.store
                    .select_story_summary_if_present(story_id, summary.id),
            )?;
        }
        Ok(report)
    }

    pub fn today(&self, now: DateTime<Utc>) -> Result<TodayView> {
        let briefing = storage_result(self.store.load_latest_briefing())?
            .ok_or_else(|| SignalError::NotFound("No briefing is stored".to_owned()))?;
        let age_seconds = now
            .signed_duration_since(briefing.generated_at)
            .num_seconds();
        let stale_after_seconds = self.config.briefing.stale_after_minutes.saturating_mul(60);
        let is_stale = u64::try_from(age_seconds).is_ok_and(|age| age > stale_after_seconds);
        Ok(TodayView { briefing, is_stale })
    }

    pub fn latest(&self, limit: usize) -> Result<Vec<Story>> {
        let mut stories = storage_result(self.store.list_latest())?;
        stories.truncate(limit);
        Ok(stories)
    }

    pub fn show(&self, id: &str) -> Result<Story> {
        storage_result(self.store.find_story(id))?
            .ok_or_else(|| SignalError::NotFound("Story was not found".to_owned()))
    }

    pub fn set_saved(&self, id: &str, saved: bool) -> Result<Story> {
        self.show(id)?;
        storage_result(self.store.set_saved(id, saved))?;
        self.show(id)
    }

    pub fn saved(&self) -> Result<Vec<Story>> {
        storage_result(self.store.list_saved())
    }

    pub fn status(&self) -> Result<StoreStatus> {
        storage_result(self.store.status())
    }

    pub fn list_sources(&self) -> Vec<Source> {
        self.config.sources.clone()
    }

    pub fn set_source_enabled(&mut self, id: &str, enabled: bool) -> Result<Source> {
        let source = self
            .config
            .sources
            .iter_mut()
            .find(|source| source.id == id)
            .ok_or_else(|| SignalError::NotFound("Source was not found".to_owned()))?;
        source.enabled = enabled;
        let updated = source.clone();
        ConfigRepository::new(self.paths.clone()).save(&self.config)?;
        Ok(updated)
    }

    fn coordinator(&self) -> AiGenerationCoordinator<'_> {
        AiGenerationCoordinator::new(
            &self.store,
            self.credential_store.as_ref(),
            self.environment_reader.as_ref(),
            self.provider_registry.as_ref(),
        )
    }

    fn find_model_selector(&self, selector: &str) -> Result<ModelProfile> {
        let selector = selector.trim();
        let by_id = Uuid::parse_str(selector)
            .ok()
            .map(|id| self.store.find_model_profile(id))
            .transpose()?
            .flatten();
        by_id
            .or(storage_result(
                self.store.find_model_profile_by_name(selector),
            )?)
            .ok_or_else(|| SignalError::NotFound("Model profile was not found".to_owned()))
    }
}

fn merge_partial_briefing(
    fresh: &mut Briefing,
    previous: Option<&Briefing>,
    failures: &[SourceFailure],
    max_items: usize,
) {
    fresh.items.truncate(max_items);
    for item in &mut fresh.items {
        item.is_stale = false;
    }

    let failed_source_ids = failures
        .iter()
        .map(|failure| failure.source_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut story_ids = fresh
        .items
        .iter()
        .map(|item| item.story.id.clone())
        .collect::<BTreeSet<_>>();
    let mut canonical_urls = fresh
        .items
        .iter()
        .map(|item| item.story.canonical_url.clone())
        .collect::<BTreeSet<_>>();

    if let Some(previous) = previous {
        for previous_item in &previous.items {
            if fresh.items.len() >= max_items {
                break;
            }
            let belongs_to_failed_source = previous_item
                .story
                .source_ids
                .iter()
                .any(|source_id| failed_source_ids.contains(source_id.as_str()));
            let duplicates_fresh = story_ids.contains(&previous_item.story.id)
                || canonical_urls.contains(&previous_item.story.canonical_url);
            if !belongs_to_failed_source || duplicates_fresh {
                continue;
            }

            let mut carried = previous_item.clone();
            carried.is_stale = true;
            story_ids.insert(carried.story.id.clone());
            canonical_urls.insert(carried.story.canonical_url.clone());
            fresh.items.push(carried);
        }
    }

    for (index, item) in fresh.items.iter_mut().enumerate() {
        item.position = u32::try_from(index + 1).unwrap_or(u32::MAX);
    }
}

fn storage_result<T>(result: Result<T>) -> Result<T> {
    result.map_err(|error| match error {
        SignalError::Io(_) | SignalError::Serialization(_) => {
            SignalError::Storage("local data could not be read or written".to_owned())
        }
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::{BriefingItem, SourceFailure, test_support};

    #[test]
    fn default_model_profile_reflects_use_and_removal() {
        let fixture = test_support::ai_app_fixture();
        let added = fixture
            .app
            .add_model(
                AddModelInput {
                    name: "Second profile".to_owned(),
                    provider: ProviderKind::Anthropic,
                    model: "opaque-model".to_owned(),
                    endpoint: None,
                    dialect: None,
                    credential: AddModelCredential::Environment {
                        variable: "SECOND_PROFILE_KEY".to_owned(),
                    },
                    consented_at: Some(fixture.now),
                    enabled: true,
                    limits: ProfileLimits::default(),
                },
                fixture.now,
            )
            .unwrap()
            .profile;

        fixture.app.use_model("second PROFILE").unwrap();
        assert_eq!(
            fixture.app.default_model_profile().unwrap().unwrap().id,
            added.id
        );

        fixture.app.remove_model("Second profile").unwrap();
        assert!(fixture.app.default_model_profile().unwrap().is_none());
    }

    #[test]
    fn today_loads_yesterdays_latest_briefing_and_reports_staleness() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(temp.path());
        let mut config = test_support::config_fixture();
        config.briefing.stale_after_minutes = 60;
        let store = Store::open(paths.data_dir.join("signal.sqlite3")).unwrap();
        let now = test_support::fixed_now();
        let mut briefing = test_support::briefing_fixture();
        briefing.date = (now - Duration::days(1)).date_naive();
        briefing.generated_at = now - Duration::days(1);
        let stories = briefing
            .items
            .iter()
            .map(|item| item.story.clone())
            .collect::<Vec<_>>();
        store.commit_refresh(&stories, &briefing).unwrap();
        let app = SignalApp {
            paths,
            config,
            store,
            credential_store: Arc::new(test_support::MemoryCredentialStore::default()),
            environment_reader: Arc::new(test_support::MemoryEnvironmentReader::default()),
            provider_registry: Arc::new(ProviderRegistry::new()),
        };

        let view = app.today(now).unwrap();

        assert_eq!(view.briefing.date, briefing.date);
        assert!(view.is_stale);
    }

    #[test]
    fn partial_merge_carries_only_missing_failed_source_items_with_stable_positions() {
        let now = test_support::fixed_now();
        let mut fresh_story = test_support::story_fixture("fresh-story");
        fresh_story.canonical_url = "https://example.com/shared".to_owned();
        fresh_story.source_ids = vec!["successful-source".to_owned()];
        fresh_story.title = "Fresh updated story".to_owned();
        let mut fresh = Briefing {
            date: now.date_naive(),
            generated_at: now,
            items: vec![BriefingItem {
                position: 1,
                section: "top_signals".to_owned(),
                is_stale: false,
                story: fresh_story,
                selected_summary: None,
            }],
        };
        let mut carried_story = test_support::story_fixture("failed-story");
        carried_story.source_ids = vec!["failed-source".to_owned()];
        carried_story.title = "Carried failed-source story".to_owned();
        let mut duplicate_story = test_support::story_fixture("old-duplicate");
        duplicate_story.canonical_url = "https://example.com/shared".to_owned();
        duplicate_story.source_ids = vec!["failed-source".to_owned()];
        duplicate_story.title = "Old duplicate".to_owned();
        let previous = Briefing {
            date: (now - Duration::days(1)).date_naive(),
            generated_at: now - Duration::days(1),
            items: vec![
                BriefingItem {
                    position: 1,
                    section: "top_signals".to_owned(),
                    is_stale: false,
                    story: carried_story,
                    selected_summary: None,
                },
                BriefingItem {
                    position: 2,
                    section: "top_signals".to_owned(),
                    is_stale: false,
                    story: duplicate_story,
                    selected_summary: None,
                },
            ],
        };
        let failures = vec![SourceFailure {
            source_id: "failed-source".to_owned(),
            message: "source could not be collected".to_owned(),
        }];

        merge_partial_briefing(&mut fresh, Some(&previous), &failures, 2);

        assert_eq!(fresh.items.len(), 2);
        assert_eq!(fresh.items[0].position, 1);
        assert_eq!(fresh.items[0].story.title, "Fresh updated story");
        assert!(!fresh.items[0].is_stale);
        assert_eq!(fresh.items[1].position, 2);
        assert_eq!(fresh.items[1].story.title, "Carried failed-source story");
        assert!(fresh.items[1].is_stale);
        assert!(
            fresh
                .items
                .iter()
                .all(|item| item.story.title != "Old duplicate")
        );
    }
}
