mod openai;
mod parse;
mod retry;

use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use reqwest::{Client, Response, StatusCode, redirect};
use serde::de::DeserializeOwned;
use url::Url;

use crate::{
    AiSummaryFields, ApiDialect, GenerationFailureKind, ModelProfile, ProviderKind,
    ResolvedCredential, SignalError,
};

pub use openai::OpenAiProvider;
pub use parse::{
    AI_SUMMARY_PROMPT_VERSION, AiSummaryPrompt, build_ai_summary_prompt, parse_ai_summary,
};
pub use retry::{
    RetryAttemptFailure, RetryPolicy, RetrySleeper, TokioRetrySleeper, retry_provider_operation,
};

const PROVIDER_USER_AGENT: &str = "ai-daily-signal/0.1.0";
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
static PROVIDER_HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderRequest {
    pub(crate) story_id: String,
    pub(crate) model: String,
    pub(crate) endpoint: Option<Url>,
    pub(crate) dialect: Option<ApiDialect>,
    pub(crate) system_text: String,
    pub(crate) user_text: String,
    pub(crate) timeout: Duration,
    pub(crate) max_output_tokens: u32,
    pub(crate) max_retries: u32,
}

impl ProviderRequest {
    pub fn from_profile(
        story_id: impl Into<String>,
        profile: &ModelProfile,
        prompt: AiSummaryPrompt,
    ) -> crate::Result<Self> {
        if profile.endpoint.as_ref().is_some_and(|endpoint| {
            !endpoint.username().is_empty() || endpoint.password().is_some()
        }) {
            return Err(SignalError::InvalidConfiguration(
                "provider request endpoint is invalid".to_owned(),
            ));
        }

        Ok(Self {
            story_id: story_id.into(),
            model: profile.model.trim().to_owned(),
            endpoint: profile.endpoint.clone(),
            dialect: profile.dialect,
            system_text: prompt.system_text,
            user_text: prompt.user_text,
            timeout: Duration::from_secs(profile.limits.timeout_seconds),
            max_output_tokens: profile.limits.max_output_tokens,
            max_retries: profile.limits.max_retries,
        })
    }
}

impl fmt::Debug for ProviderRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderRequest")
            .field("timeout", &self.timeout)
            .field("max_output_tokens", &self.max_output_tokens)
            .field("max_retries", &self.max_retries)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderResponse {
    pub fields: AiSummaryFields,
    pub usage: Option<ProviderUsage>,
}

#[async_trait::async_trait]
pub trait SummaryProvider: Send + Sync {
    async fn generate(
        &self,
        request: &ProviderRequest,
        credential: &ResolvedCredential,
    ) -> Result<ProviderResponse, ProviderFailure>;
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: Vec<(ProviderKind, Arc<dyn SummaryProvider>)>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, kind: ProviderKind, provider: Arc<dyn SummaryProvider>) {
        if let Some((_, existing)) = self
            .providers
            .iter_mut()
            .find(|(registered, _)| *registered == kind)
        {
            *existing = provider;
        } else {
            self.providers.push((kind, provider));
        }
    }

    pub fn provider(&self, kind: ProviderKind) -> Option<Arc<dyn SummaryProvider>> {
        self.providers
            .iter()
            .find(|(registered, _)| *registered == kind)
            .map(|(_, provider)| Arc::clone(provider))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderFailureKind {
    CredentialMissing,
    Authentication,
    RateLimited,
    Timeout,
    Transport,
    ProviderRejected,
    ProviderUnavailable,
    MalformedOutput,
}

impl ProviderFailureKind {
    pub fn from_http_status(status: StatusCode) -> Self {
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Self::Authentication,
            StatusCode::TOO_MANY_REQUESTS => Self::RateLimited,
            status if status.is_server_error() => Self::ProviderUnavailable,
            _ => Self::ProviderRejected,
        }
    }

    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::RateLimited | Self::ProviderUnavailable
        )
    }
}

impl From<ProviderFailureKind> for GenerationFailureKind {
    fn from(value: ProviderFailureKind) -> Self {
        match value {
            ProviderFailureKind::CredentialMissing => Self::CredentialMissing,
            ProviderFailureKind::Authentication => Self::Authentication,
            ProviderFailureKind::RateLimited => Self::RateLimited,
            ProviderFailureKind::Timeout => Self::Timeout,
            ProviderFailureKind::Transport => Self::Transport,
            ProviderFailureKind::ProviderRejected => Self::ProviderRejected,
            ProviderFailureKind::ProviderUnavailable => Self::ProviderUnavailable,
            ProviderFailureKind::MalformedOutput => Self::MalformedOutput,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestChargeStatus {
    NotSent,
    PossiblySent,
}

impl RequestChargeStatus {
    fn combine(self, other: Self) -> Self {
        if matches!(self, Self::PossiblySent) || matches!(other, Self::PossiblySent) {
            Self::PossiblySent
        } else {
            Self::NotSent
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderFailure {
    kind: ProviderFailureKind,
    charge_status: RequestChargeStatus,
}

impl ProviderFailure {
    pub const fn new(kind: ProviderFailureKind, charge_status: RequestChargeStatus) -> Self {
        Self {
            kind,
            charge_status,
        }
    }

    pub const fn kind(self) -> ProviderFailureKind {
        self.kind
    }

    pub const fn charge_status(self) -> RequestChargeStatus {
        self.charge_status
    }

    fn with_charge_status(self, charge_status: RequestChargeStatus) -> Self {
        Self {
            kind: self.kind,
            charge_status,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn for_test(_credential: &str, _response_body: &str) -> Self {
        Self::new(
            ProviderFailureKind::MalformedOutput,
            RequestChargeStatus::PossiblySent,
        )
    }
}

impl fmt::Display for ProviderFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AI provider request failed")
    }
}

impl std::error::Error for ProviderFailure {}

pub(crate) fn shared_http_client() -> Result<Client, ProviderFailure> {
    if let Some(client) = PROVIDER_HTTP_CLIENT.get() {
        return Ok(client.clone());
    }

    let client = Client::builder()
        .redirect(redirect::Policy::none())
        .user_agent(PROVIDER_USER_AGENT)
        .build()
        .map_err(|_| {
            ProviderFailure::new(ProviderFailureKind::Transport, RequestChargeStatus::NotSent)
        })?;
    let _ = PROVIDER_HTTP_CLIENT.set(client.clone());
    Ok(PROVIDER_HTTP_CLIENT.get().cloned().unwrap_or(client))
}

pub(crate) async fn read_json_response<T: DeserializeOwned>(
    mut response: Response,
) -> Result<T, ProviderFailure> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(response_failure(ProviderFailureKind::MalformedOutput));
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        response_failure(if error.is_timeout() {
            ProviderFailureKind::Timeout
        } else {
            ProviderFailureKind::Transport
        })
    })? {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(response_failure(ProviderFailureKind::MalformedOutput));
        }
        bytes.extend_from_slice(&chunk);
    }

    serde_json::from_slice(&bytes)
        .map_err(|_| response_failure(ProviderFailureKind::MalformedOutput))
}

fn response_failure(kind: ProviderFailureKind) -> ProviderFailure {
    ProviderFailure::new(kind, RequestChargeStatus::PossiblySent)
}
