use std::{fs, io::Write};

use sha2::Digest;

use crate::{AppPaths, Result, SignalError, Source};
use atomic_write_file::AtomicWriteFile;

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

impl ConfigRepository {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub fn load_or_create(&self) -> Result<AppConfig> {
        let config_path = self.paths.config_dir.join("config.toml");
        if config_path.exists() {
            return self.load();
        }

        let config = Self::parse(include_str!("../assets/standard-sources.toml"))?;
        self.save(&config)?;
        Ok(config)
    }

    pub fn load(&self) -> Result<AppConfig> {
        Self::parse(&fs::read_to_string(
            self.paths.config_dir.join("config.toml"),
        )?)
    }

    pub fn revision(&self) -> Result<String> {
        let bytes = fs::read(self.paths.config_dir.join("config.toml"))?;
        Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        fs::create_dir_all(&self.paths.config_dir)?;

        let config_path = self.paths.config_dir.join("config.toml");
        let serialized = toml::to_string_pretty(config)
            .map_err(|error| SignalError::Serialization(error.to_string()))?;
        let mut config_file = AtomicWriteFile::open(config_path)?;
        config_file.write_all(serialized.as_bytes())?;
        config_file.flush()?;
        config_file.commit()?;
        Ok(())
    }

    fn parse(contents: &str) -> Result<AppConfig> {
        toml::from_str(contents).map_err(|error| SignalError::Serialization(error.to_string()))
    }
}
