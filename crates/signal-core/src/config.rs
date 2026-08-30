use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    time::{Duration, Instant},
};

use fs2::FileExt;
use sha2::Digest;
use url::Url;

use crate::{AppPaths, Result, SignalError, Source, SourceKind};
use atomic_write_file::AtomicWriteFile;

pub const CONFIG_LOCK_FILE_NAME: &str = "config.toml.lock";
const CONFIG_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const CONFIG_LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct BriefingConfig {
    pub max_items: usize,
    pub stale_after_minutes: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AppConfig {
    pub briefing: BriefingConfig,
    pub sources: Vec<Source>,
}

pub struct ConfigRepository {
    paths: AppPaths,
}

pub struct LoadedConfig {
    pub config: AppConfig,
    pub revision: String,
}

pub(crate) struct ConfigMutation<T> {
    pub value: T,
    pub config: AppConfig,
    pub revision: String,
}

#[derive(Clone, Copy)]
enum LockMode {
    Shared,
    Exclusive,
}

struct ConfigLock {
    _file: File,
}

impl ConfigRepository {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub fn load_or_create(&self) -> Result<AppConfig> {
        let _lock = self.acquire_lock(LockMode::Exclusive)?;
        let config_path = self.paths.config_dir.join("config.toml");
        if config_path.exists() {
            return Ok(self.load_unlocked()?.config);
        }

        let config = Self::parse(include_str!("../assets/standard-sources.toml"))?;
        let bytes = Self::serialize(&config)?;
        self.save_bytes_unlocked(&bytes)?;
        Ok(config)
    }

    pub fn load(&self) -> Result<AppConfig> {
        Ok(self.load_with_revision()?.config)
    }

    pub fn load_with_revision(&self) -> Result<LoadedConfig> {
        let _lock = self.acquire_lock(LockMode::Shared)?;
        self.load_unlocked()
    }

    pub fn revision(&self) -> Result<String> {
        Ok(self.load_with_revision()?.revision)
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let _lock = self.acquire_lock(LockMode::Exclusive)?;
        let bytes = Self::serialize(config)?;
        self.save_bytes_unlocked(&bytes)
    }

    pub(crate) fn mutate<T>(
        &self,
        mutation: impl FnOnce(&mut AppConfig) -> Result<T>,
    ) -> Result<ConfigMutation<T>> {
        let _lock = self.acquire_lock(LockMode::Exclusive)?;
        let mut config = self.load_unlocked()?.config;
        let value = mutation(&mut config)?;
        let bytes = Self::serialize(&config)?;
        let revision = revision_for(&bytes);
        self.save_bytes_unlocked(&bytes)?;
        Ok(ConfigMutation {
            value,
            config,
            revision,
        })
    }

    fn acquire_lock(&self, mode: LockMode) -> Result<ConfigLock> {
        fs::create_dir_all(&self.paths.config_dir)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.paths.config_dir.join(CONFIG_LOCK_FILE_NAME))?;
        let deadline = Instant::now() + CONFIG_LOCK_TIMEOUT;
        loop {
            let result = match mode {
                LockMode::Shared => FileExt::try_lock_shared(&file),
                LockMode::Exclusive => FileExt::try_lock_exclusive(&file),
            };
            match result {
                Ok(()) => return Ok(ConfigLock { _file: file }),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(SignalError::Storage(
                            "source configuration is busy".to_owned(),
                        ));
                    }
                    std::thread::sleep(CONFIG_LOCK_RETRY_INTERVAL);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn load_unlocked(&self) -> Result<LoadedConfig> {
        let bytes = fs::read(self.paths.config_dir.join("config.toml"))?;
        let contents = std::str::from_utf8(&bytes).map_err(|_| {
            SignalError::Serialization("configuration is not valid UTF-8".to_owned())
        })?;
        Ok(LoadedConfig {
            config: Self::parse(contents)?,
            revision: revision_for(&bytes),
        })
    }

    fn save_bytes_unlocked(&self, bytes: &[u8]) -> Result<()> {
        fs::create_dir_all(&self.paths.config_dir)?;

        let config_path = self.paths.config_dir.join("config.toml");
        let mut config_file = AtomicWriteFile::open(config_path)?;
        config_file.write_all(bytes)?;
        config_file.flush()?;
        config_file.commit()?;
        Ok(())
    }

    pub(crate) fn standard_source_ids() -> Result<BTreeSet<String>> {
        Ok(
            Self::parse(include_str!("../assets/standard-sources.toml"))?
                .sources
                .into_iter()
                .map(|source| source.id)
                .collect(),
        )
    }

    fn parse(contents: &str) -> Result<AppConfig> {
        let config = toml::from_str(contents)
            .map_err(|error| SignalError::Serialization(error.to_string()))?;
        Self::validate(&config)?;
        Ok(config)
    }

    fn serialize(config: &AppConfig) -> Result<Vec<u8>> {
        Self::validate(config)?;
        toml::to_string_pretty(config)
            .map(String::into_bytes)
            .map_err(|error| SignalError::Serialization(error.to_string()))
    }

    fn validate(config: &AppConfig) -> Result<()> {
        if config.briefing.max_items == 0 {
            return Err(SignalError::InvalidConfiguration(
                "briefing max_items must be greater than zero".to_owned(),
            ));
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for source in &config.sources {
            if source.id.trim().is_empty()
                || source.name.trim().is_empty()
                || source.category.trim().is_empty()
                || !source.weight.is_finite()
                || !(0.0..=1.0).contains(&source.weight)
            {
                return Err(SignalError::InvalidConfiguration(
                    "source configuration is invalid".to_owned(),
                ));
            }
            if !ids.insert(source.id.clone()) || !names.insert(source.name.trim().to_lowercase()) {
                return Err(SignalError::InvalidConfiguration(
                    "source identifiers and names must be unique".to_owned(),
                ));
            }
            let SourceKind::Feed { url } = &source.kind;
            let parsed = Url::parse(url).map_err(|_| {
                SignalError::InvalidConfiguration("source URL must be valid".to_owned())
            })?;
            if !matches!(parsed.scheme(), "http" | "https")
                || parsed.host_str().is_none()
                || has_user_info_delimiter(url, &parsed)
            {
                return Err(SignalError::InvalidConfiguration(
                    "source URL must be HTTP or HTTPS with a host and no user info".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

fn revision_for(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn has_user_info_delimiter(input: &str, url: &Url) -> bool {
    let Some((input_scheme, after_scheme)) = input.split_once(':') else {
        return false;
    };
    if !input_scheme.eq_ignore_ascii_case(url.scheme()) {
        return false;
    }
    after_scheme
        .strip_prefix("//")
        .and_then(|authority_and_path| authority_and_path.split(['/', '?', '#']).next())
        .is_some_and(|authority| authority.contains('@'))
}
