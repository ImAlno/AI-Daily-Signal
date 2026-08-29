use std::net::IpAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::{Result, SignalError};

pub const SYSTEM_STORE_SERVICE: &str = "com.AIDailySignal.signal";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
    Gemini,
    OpenAiCompatible,
}

impl ProviderKind {
    pub(crate) fn as_storage(self) -> &'static str {
        match self {
            Self::OpenAi => "open_ai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::OpenAiCompatible => "open_ai_compatible",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self> {
        match value {
            "open_ai" => Ok(Self::OpenAi),
            "anthropic" => Ok(Self::Anthropic),
            "gemini" => Ok(Self::Gemini),
            "open_ai_compatible" => Ok(Self::OpenAiCompatible),
            _ => Err(SignalError::Serialization(format!(
                "invalid provider kind {value:?}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiDialect {
    Responses,
    ChatCompletions,
}

impl ApiDialect {
    pub(crate) fn as_storage(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ChatCompletions => "chat_completions",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self> {
        match value {
            "responses" => Ok(Self::Responses),
            "chat_completions" => Ok(Self::ChatCompletions),
            _ => Err(SignalError::Serialization(format!(
                "invalid API dialect {value:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CredentialRef {
    SystemStore { service: String, account: String },
    Environment { variable: String },
}

impl CredentialRef {
    pub fn for_profile(id: Uuid) -> Self {
        Self::SystemStore {
            service: SYSTEM_STORE_SERVICE.to_owned(),
            account: format!("model-profile/{}", id.hyphenated()),
        }
    }

    fn validate(&self, profile_id: Option<Uuid>) -> Result<()> {
        match self {
            Self::SystemStore { service, account } => {
                if service != SYSTEM_STORE_SERVICE {
                    return invalid("system-store credentials must use the application service");
                }
                let Some(uuid) = account.strip_prefix("model-profile/") else {
                    return invalid("system-store credential account is invalid");
                };
                let parsed = Uuid::parse_str(uuid).map_err(|_| {
                    SignalError::InvalidConfiguration(
                        "system-store credential account is invalid".to_owned(),
                    )
                })?;
                if uuid != parsed.hyphenated().to_string()
                    || profile_id.is_some_and(|profile_id| profile_id != parsed)
                {
                    return invalid("system-store credential account is invalid");
                }
                Ok(())
            }
            Self::Environment { variable } if valid_environment_variable(variable) => Ok(()),
            Self::Environment { .. } => invalid("environment credential variable is invalid"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileLimits {
    pub max_summaries_per_refresh: u32,
    pub max_daily_cost_microusd: Option<u64>,
    pub input_cost_microusd_per_million: Option<u64>,
    pub output_cost_microusd_per_million: Option<u64>,
    pub max_output_tokens: u32,
    pub timeout_seconds: u64,
    pub max_retries: u32,
}

impl Default for ProfileLimits {
    fn default() -> Self {
        Self {
            max_summaries_per_refresh: 5,
            max_daily_cost_microusd: None,
            input_cost_microusd_per_million: None,
            output_cost_microusd_per_million: None,
            max_output_tokens: 384,
            timeout_seconds: 30,
            max_retries: 2,
        }
    }
}

impl ProfileLimits {
    fn validate(&self) -> Result<()> {
        if self.max_summaries_per_refresh == 0
            || self.max_output_tokens == 0
            || self.timeout_seconds == 0
            || self.max_retries == 0
        {
            return invalid("profile limits must be nonzero");
        }

        match (
            self.input_cost_microusd_per_million,
            self.output_cost_microusd_per_million,
        ) {
            (None, None) => {
                if self.max_daily_cost_microusd.is_some() {
                    invalid("daily monetary limits require input and output rates")
                } else {
                    Ok(())
                }
            }
            (Some(input), Some(output)) if input > 0 && output > 0 => Ok(()),
            (Some(_), Some(_)) => invalid("model rates must be nonzero"),
            _ => invalid("input and output model rates must be configured together"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewModelProfile {
    pub name: String,
    pub provider: ProviderKind,
    pub model: String,
    pub endpoint: Option<Url>,
    pub dialect: Option<ApiDialect>,
    pub credential: CredentialRef,
    pub consented_at: Option<DateTime<Utc>>,
    pub enabled: bool,
    pub limits: ProfileLimits,
}

impl NewModelProfile {
    pub fn validate(&self) -> Result<()> {
        ProfileFields::from_new(self).validate(None)
    }

    pub fn into_model_profile(
        self,
        id: Uuid,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<ModelProfile> {
        let profile = ModelProfile {
            id,
            name: self.name.trim().to_owned(),
            provider: self.provider,
            model: self.model.trim().to_owned(),
            endpoint: self.endpoint,
            dialect: self.dialect,
            credential: self.credential,
            consented_at: self.consented_at,
            enabled: self.enabled,
            limits: self.limits,
            created_at,
            updated_at,
        };
        profile.validate()?;
        Ok(profile)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelProfile {
    pub id: Uuid,
    pub name: String,
    pub provider: ProviderKind,
    pub model: String,
    pub endpoint: Option<Url>,
    pub dialect: Option<ApiDialect>,
    pub credential: CredentialRef,
    pub consented_at: Option<DateTime<Utc>>,
    pub enabled: bool,
    pub limits: ProfileLimits,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ModelProfile {
    pub fn validate(&self) -> Result<()> {
        ProfileFields::from_model(self).validate(Some(self.id))
    }

    pub(crate) fn normalized(&self) -> Self {
        let mut profile = self.clone();
        profile.name = profile.name.trim().to_owned();
        profile.model = profile.model.trim().to_owned();
        profile
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct MoneyMicros(u64);

impl MoneyMicros {
    pub fn parse_usd(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() || value.starts_with(['+', '-']) {
            return invalid("USD amount is invalid");
        }

        let mut parts = value.split('.');
        let whole = parts.next().unwrap_or_default();
        let fraction = parts.next();
        if parts.next().is_some()
            || (whole.is_empty() && fraction.is_none())
            || !whole.chars().all(|character| character.is_ascii_digit())
        {
            return invalid("USD amount is invalid");
        }
        let whole: u64 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| {
                SignalError::InvalidConfiguration("USD amount is invalid".to_owned())
            })?
        };
        let fraction = match fraction {
            None => 0,
            Some(value)
                if !value.is_empty()
                    && value.len() <= 6
                    && value.chars().all(|character| character.is_ascii_digit()) =>
            {
                let digits: u64 = value.parse().map_err(|_| {
                    SignalError::InvalidConfiguration("USD amount is invalid".to_owned())
                })?;
                digits
                    .checked_mul(10_u64.pow((6 - value.len()) as u32))
                    .ok_or_else(|| {
                        SignalError::InvalidConfiguration("USD amount is invalid".to_owned())
                    })?
            }
            _ => return invalid("USD amount is invalid"),
        };
        whole
            .checked_mul(1_000_000)
            .and_then(|value| value.checked_add(fraction))
            .map(Self)
            .ok_or_else(|| SignalError::InvalidConfiguration("USD amount is invalid".to_owned()))
    }

    pub fn as_micros(self) -> u64 {
        self.0
    }
}

struct ProfileFields<'a> {
    name: &'a str,
    provider: ProviderKind,
    model: &'a str,
    endpoint: Option<&'a Url>,
    dialect: Option<ApiDialect>,
    credential: &'a CredentialRef,
    limits: &'a ProfileLimits,
}

impl<'a> ProfileFields<'a> {
    fn from_new(profile: &'a NewModelProfile) -> Self {
        Self {
            name: &profile.name,
            provider: profile.provider,
            model: &profile.model,
            endpoint: profile.endpoint.as_ref(),
            dialect: profile.dialect,
            credential: &profile.credential,
            limits: &profile.limits,
        }
    }

    fn from_model(profile: &'a ModelProfile) -> Self {
        Self {
            name: &profile.name,
            provider: profile.provider,
            model: &profile.model,
            endpoint: profile.endpoint.as_ref(),
            dialect: profile.dialect,
            credential: &profile.credential,
            limits: &profile.limits,
        }
    }

    fn validate(&self, profile_id: Option<Uuid>) -> Result<()> {
        if self.name.trim().is_empty() || self.model.trim().is_empty() {
            return invalid("profile name and model are required");
        }
        match self.provider {
            ProviderKind::OpenAi | ProviderKind::Anthropic | ProviderKind::Gemini
                if self.endpoint.is_none() && self.dialect.is_none() => {}
            ProviderKind::OpenAi | ProviderKind::Anthropic | ProviderKind::Gemini => {
                return invalid("official providers do not accept custom endpoint or dialect");
            }
            ProviderKind::OpenAiCompatible => {
                let (Some(endpoint), Some(_)) = (self.endpoint, self.dialect) else {
                    return invalid("custom providers require an endpoint and API dialect");
                };
                validate_endpoint(endpoint)?;
            }
        }
        self.credential.validate(profile_id)?;
        self.limits.validate()
    }
}

fn validate_endpoint(endpoint: &Url) -> Result<()> {
    if !endpoint.username().is_empty() || endpoint.password().is_some() {
        return invalid("custom endpoint must not contain user info");
    }
    let Some(host) = endpoint.host_str() else {
        return invalid("custom endpoint must have a host");
    };
    if endpoint.scheme() != "https" && !(endpoint.scheme() == "http" && is_loopback(host)) {
        return invalid("custom endpoint must use HTTPS unless it is loopback HTTP");
    }
    Ok(())
}

fn is_loopback(host: &str) -> bool {
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn valid_environment_variable(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some('_' | 'A'..='Z' | 'a'..='z'))
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn invalid<T>(message: &str) -> Result<T> {
    Err(SignalError::InvalidConfiguration(message.to_owned()))
}
