use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    ApiDialect, ModelProfile, ProviderKind, Result, SignalError, Story, normalize_title,
    normalize_url,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AiSummaryFields {
    pub what_happened: String,
    pub why_it_matters: String,
    pub caveat: Option<String>,
}

impl AiSummaryFields {
    pub fn validate(&self, settings: &SummarySettings) -> Result<()> {
        validate_scalar(
            "what_happened",
            &self.what_happened,
            settings.what_happened_max_chars,
            true,
        )?;
        validate_scalar(
            "why_it_matters",
            &self.why_it_matters,
            settings.why_it_matters_max_chars,
            true,
        )?;
        if let Some(caveat) = &self.caveat {
            validate_scalar("caveat", caveat, settings.caveat_max_chars, true)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SummarySettings {
    pub what_happened_max_chars: u32,
    pub why_it_matters_max_chars: u32,
    pub caveat_max_chars: u32,
}

impl Default for SummarySettings {
    fn default() -> Self {
        Self {
            what_happened_max_chars: 600,
            why_it_matters_max_chars: 600,
            caveat_max_chars: 300,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SummaryVariant {
    pub id: Uuid,
    pub story_id: String,
    pub profile_id: Option<Uuid>,
    pub provider: ProviderKind,
    pub model: String,
    pub endpoint: Option<String>,
    pub dialect: Option<ApiDialect>,
    pub prompt_version: String,
    pub cache_key: String,
    pub fields: AiSummaryFields,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_microusd: u64,
    pub generated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenerationStatus {
    Reserved,
    Completed,
    Failed,
}

impl GenerationStatus {
    pub(crate) fn as_storage(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self> {
        match value {
            "reserved" => Ok(Self::Reserved),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Err(SignalError::Serialization(format!(
                "invalid generation status {value:?}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenerationFailureKind {
    CredentialMissing,
    Authentication,
    RateLimited,
    Timeout,
    Transport,
    ProviderRejected,
    ProviderUnavailable,
    MalformedOutput,
}

impl GenerationFailureKind {
    pub(crate) fn as_storage(self) -> &'static str {
        match self {
            Self::CredentialMissing => "credential_missing",
            Self::Authentication => "authentication",
            Self::RateLimited => "rate_limited",
            Self::Timeout => "timeout",
            Self::Transport => "transport",
            Self::ProviderRejected => "provider_rejected",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::MalformedOutput => "malformed_output",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self> {
        match value {
            "credential_missing" => Ok(Self::CredentialMissing),
            "authentication" => Ok(Self::Authentication),
            "rate_limited" => Ok(Self::RateLimited),
            "timeout" => Ok(Self::Timeout),
            "transport" => Ok(Self::Transport),
            "provider_rejected" => Ok(Self::ProviderRejected),
            "provider_unavailable" => Ok(Self::ProviderUnavailable),
            "malformed_output" => Ok(Self::MalformedOutput),
            _ => Err(SignalError::Serialization(format!(
                "invalid generation failure kind {value:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationAttempt {
    pub id: Uuid,
    pub profile_id: Option<Uuid>,
    pub provider: ProviderKind,
    pub model: String,
    pub endpoint: Option<String>,
    pub dialect: Option<ApiDialect>,
    pub usage_date: NaiveDate,
    pub status: GenerationStatus,
    pub estimated_cost_microusd: u64,
    pub actual_cost_microusd: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub failure_kind: Option<GenerationFailureKind>,
    pub reserved_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub finalized_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AttemptOutcome {
    Completed {
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        cost_microusd: u64,
    },
    FailedCharged {
        category: GenerationFailureKind,
        cost_microusd: u64,
    },
    FailedUncharged {
        category: GenerationFailureKind,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BudgetReservation {
    pub attempt_id: Uuid,
    pub profile_id: Uuid,
    pub usage_date: NaiveDate,
    pub estimated_cost_microusd: u64,
    pub reserved_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum BudgetDecision {
    Reserved(BudgetReservation),
    Exhausted,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationReport {
    pub eligible: usize,
    pub generated: usize,
    pub cache_hits: usize,
    pub skipped_cap: usize,
    pub skipped_budget: usize,
    pub missing_credentials: usize,
    pub provider_failures: usize,
    pub malformed_outputs: usize,
    pub smart_fallbacks: usize,
}

#[derive(Serialize)]
struct CacheIdentity<'a> {
    story: CacheStoryIdentity,
    provider: &'static str,
    endpoint: Option<String>,
    model: &'a str,
    dialect: Option<&'static str>,
    prompt_version: &'a str,
    max_output_tokens: u32,
    settings: &'a SummarySettings,
}

#[derive(Serialize)]
struct CacheStoryIdentity {
    normalized_title: String,
    excerpt: String,
    canonical_url: String,
    published_at: Option<String>,
    category: String,
    source_ids: Vec<String>,
}

pub fn summary_cache_key(
    story: &Story,
    profile: &ModelProfile,
    prompt_version: &str,
    settings: &SummarySettings,
) -> Result<String> {
    let mut source_ids = story
        .source_ids
        .iter()
        .map(|value| collapse_whitespace(value))
        .collect::<Vec<_>>();
    source_ids.sort();
    let identity = CacheIdentity {
        story: CacheStoryIdentity {
            normalized_title: normalize_title(&story.title),
            excerpt: collapse_whitespace(&story.excerpt),
            canonical_url: normalize_url(&story.canonical_url),
            published_at: story.published_at.map(|value| value.to_rfc3339()),
            category: collapse_whitespace(&story.category),
            source_ids,
        },
        provider: profile.provider.as_storage(),
        endpoint: profile.endpoint.as_ref().map(normalized_endpoint),
        model: &profile.model,
        dialect: profile.dialect.map(ApiDialect::as_storage),
        prompt_version,
        max_output_tokens: profile.limits.max_output_tokens,
        settings,
    };
    let canonical = serde_json::to_vec(&identity)
        .map_err(|error| SignalError::Serialization(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn normalized_endpoint(endpoint: &url::Url) -> String {
    let mut endpoint = endpoint.clone();
    endpoint.set_fragment(None);
    if let Some(host) = endpoint.host_str().map(str::to_ascii_lowercase) {
        let _ = endpoint.set_host(Some(&host));
    }
    if matches!(
        (endpoint.scheme(), endpoint.port()),
        ("http", Some(80)) | ("https", Some(443))
    ) {
        let _ = endpoint.set_port(None);
    }
    let mut query = endpoint.query_pairs().into_owned().collect::<Vec<_>>();
    query.sort();
    endpoint.set_query(None);
    if !query.is_empty() {
        endpoint.query_pairs_mut().extend_pairs(query);
    }
    let path = endpoint.path().trim_end_matches('/').to_owned();
    endpoint.set_path(if path.is_empty() { "/" } else { &path });
    endpoint.to_string()
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn validate_scalar(field: &str, value: &str, maximum: u32, required: bool) -> Result<()> {
    if (required && value.trim().is_empty())
        || value.chars().count() > maximum as usize
        || contains_html(value)
        || contains_markdown_link(value)
    {
        return Err(SignalError::InvalidConfiguration(format!(
            "invalid AI summary field {field}"
        )));
    }
    Ok(())
}

fn contains_html(value: &str) -> bool {
    value.char_indices().any(|(index, character)| {
        character == '<' && value[index + character.len_utf8()..].contains('>')
    })
}

fn contains_markdown_link(value: &str) -> bool {
    value.match_indices(']').any(|(index, _)| {
        value[..index].rfind('[').is_some()
            && value[index + 1..]
                .chars()
                .next()
                .is_some_and(|character| matches!(character, '(' | '['))
    })
}
