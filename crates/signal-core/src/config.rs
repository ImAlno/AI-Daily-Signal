use std::{fs, io::Write};

use crate::{AppPaths, Result, SignalError, Source};

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
            return Self::parse(&fs::read_to_string(config_path)?);
        }

        let config = Self::parse(include_str!("../assets/standard-sources.toml"))?;
        self.save(&config)?;
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        fs::create_dir_all(&self.paths.config_dir)?;

        let config_path = self.paths.config_dir.join("config.toml");
        let temp_path = self.paths.config_dir.join("config.toml.tmp");
        let serialized = toml::to_string_pretty(config)
            .map_err(|error| SignalError::Serialization(error.to_string()))?;
        let mut temp_file = fs::File::create(&temp_path)?;
        temp_file.write_all(serialized.as_bytes())?;
        temp_file.flush()?;
        drop(temp_file);
        fs::rename(temp_path, config_path)?;
        Ok(())
    }

    fn parse(contents: &str) -> Result<AppConfig> {
        toml::from_str(contents).map_err(|error| SignalError::Serialization(error.to_string()))
    }
}
