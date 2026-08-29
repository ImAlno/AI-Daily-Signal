use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};

use crate::{
    AppConfig, AppPaths, Briefing, ConfigRepository, FeedCollector, Pipeline, Result, SignalError,
    Source, SourceFailure, Store, StoreStatus, Story,
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RefreshReport {
    pub briefing: Briefing,
    pub successful_sources: usize,
    pub failures: Vec<SourceFailure>,
}

pub struct SignalApp {
    paths: AppPaths,
    config: AppConfig,
    store: Store,
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
        let config = ConfigRepository::new(paths.clone()).load_or_create()?;
        let store = storage_result(Store::open(paths.data_dir.join("signal.sqlite3")))?;

        Ok(Self {
            paths,
            config,
            store,
        })
    }

    pub fn init(&self) -> Result<StoreStatus> {
        storage_result(self.store.status())
    }

    pub async fn refresh(&self, now: DateTime<Utc>) -> Result<RefreshReport> {
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
        let output = Pipeline::build(collection.candidates, &self.config, now);
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
        })
    }

    pub fn today(&self, date: NaiveDate) -> Result<Briefing> {
        storage_result(self.store.load_briefing(date))?
            .ok_or_else(|| SignalError::NotFound(format!("No briefing is stored for {date}")))
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
}

fn storage_result<T>(result: Result<T>) -> Result<T> {
    result.map_err(|error| match error {
        SignalError::Io(_) | SignalError::Serialization(_) => {
            SignalError::Storage("local data could not be read or written".to_owned())
        }
        other => other,
    })
}
