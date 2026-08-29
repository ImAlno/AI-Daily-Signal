use std::time::SystemTime;

use reqwest::header::HeaderValue;
use reqwest::{Client, Response};
use serde::Deserialize;
use serde_json::json;

use crate::{ResolvedCredential, SummarySettings};

use super::{
    ProviderFailure, ProviderFailureKind, ProviderRequest, ProviderResponse, ProviderUsage,
    RequestChargeStatus, RetryAttemptFailure, RetryPolicy, SummaryProvider, TokioRetrySleeper,
    parse_ai_summary, read_json_response, retry_provider_operation, shared_http_client,
    transport_attempt_failure,
};

const OFFICIAL_ANTHROPIC_ORIGIN: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    client: Client,
    origin: String,
    sleeper: TokioRetrySleeper,
}

impl AnthropicProvider {
    pub fn official() -> Result<Self, ProviderFailure> {
        Self::with_official_origin(OFFICIAL_ANTHROPIC_ORIGIN)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn official_for_test(origin: impl AsRef<str>) -> Result<Self, ProviderFailure> {
        let origin = reqwest::Url::parse(origin.as_ref())
            .map_err(|_| not_sent(ProviderFailureKind::Transport))?;
        let valid_origin = matches!(origin.scheme(), "http" | "https")
            && origin
                .host_str()
                .is_some_and(crate::models::is_literal_loopback_ip_host)
            && origin.username().is_empty()
            && origin.password().is_none()
            && origin.path() == "/"
            && origin.query().is_none()
            && origin.fragment().is_none();
        if !valid_origin {
            return Err(not_sent(ProviderFailureKind::Transport));
        }
        Self::with_official_origin(origin.as_str())
    }

    fn with_official_origin(origin: &str) -> Result<Self, ProviderFailure> {
        let origin = origin.trim_end_matches('/');
        let endpoint = format!("{origin}/v1/messages");
        reqwest::Url::parse(&endpoint).map_err(|_| not_sent(ProviderFailureKind::Transport))?;
        Ok(Self {
            client: shared_http_client()?,
            origin: origin.to_owned(),
            sleeper: TokioRetrySleeper,
        })
    }

    fn messages_endpoint(&self) -> String {
        format!("{}/v1/messages", self.origin)
    }

    async fn send_attempt(
        &self,
        request: &ProviderRequest,
        credential: &ResolvedCredential,
    ) -> Result<ProviderResponse, RetryAttemptFailure> {
        let api_key = anthropic_api_key_header(credential.expose_secret())?;
        let body = json!({
            "model": request.model,
            "system": request.system_text,
            "messages": [{
                "role": "user",
                "content": request.user_text,
            }],
            "max_tokens": request.max_output_tokens,
            "stream": false,
        });
        let response = self
            .client
            .post(self.messages_endpoint())
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .timeout(request.timeout)
            .json(&body)
            .send()
            .await
            .map_err(transport_attempt_failure)?;
        parse_http_response(response).await
    }
}

#[async_trait::async_trait]
impl SummaryProvider for AnthropicProvider {
    async fn generate(
        &self,
        request: &ProviderRequest,
        credential: &ResolvedCredential,
    ) -> Result<ProviderResponse, ProviderFailure> {
        let policy = RetryPolicy::new(request.timeout, request.max_retries);
        retry_provider_operation(&policy, &self.sleeper, || {
            self.send_attempt(request, credential)
        })
        .await
    }
}

#[derive(Deserialize)]
struct MessageEnvelope {
    #[serde(default)]
    content: Vec<MessageContent>,
    usage: Option<MessageUsage>,
}

#[derive(Deserialize)]
struct MessageContent {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct MessageUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

async fn parse_http_response(response: Response) -> Result<ProviderResponse, RetryAttemptFailure> {
    if !response.status().is_success() {
        return Err(http_status_attempt_failure(&response));
    }

    let envelope: MessageEnvelope = read_json_response(response)
        .await
        .map_err(|failure| RetryAttemptFailure::new(failure, None))?;
    let text = envelope
        .content
        .iter()
        .filter(|item| item.kind == "text")
        .filter_map(|item| item.text.as_deref())
        .collect::<String>();
    if text.trim().is_empty() {
        return Err(malformed_attempt());
    }
    let fields = parse_ai_summary(&text, &SummarySettings::default())
        .map_err(|failure| RetryAttemptFailure::new(failure, None))?;
    let usage = envelope.usage.and_then(|usage| {
        Some(ProviderUsage {
            input_tokens: usage.input_tokens?,
            output_tokens: usage.output_tokens?,
        })
    });
    Ok(ProviderResponse { fields, usage })
}

fn http_status_attempt_failure(response: &Response) -> RetryAttemptFailure {
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| RetryPolicy::parse_retry_after(value, SystemTime::now()));
    RetryAttemptFailure::new(
        sent(ProviderFailureKind::from_http_status(response.status())),
        retry_after,
    )
}

fn malformed_attempt() -> RetryAttemptFailure {
    RetryAttemptFailure::new(sent(ProviderFailureKind::MalformedOutput), None)
}

fn sent(kind: ProviderFailureKind) -> ProviderFailure {
    ProviderFailure::new(kind, RequestChargeStatus::PossiblySent)
}

fn not_sent(kind: ProviderFailureKind) -> ProviderFailure {
    ProviderFailure::new(kind, RequestChargeStatus::NotSent)
}

fn anthropic_api_key_header(secret: &str) -> Result<HeaderValue, RetryAttemptFailure> {
    let mut value = HeaderValue::from_str(secret)
        .map_err(|_| RetryAttemptFailure::new(not_sent(ProviderFailureKind::Transport), None))?;
    value.set_sensitive(true);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::anthropic_api_key_header;

    #[test]
    fn request_construction_retains_a_sensitive_api_key_header() {
        let api_key = anthropic_api_key_header("opaque-test-key").unwrap();
        let request = reqwest::Client::new()
            .post("https://example.com/v1/messages")
            .header("x-api-key", api_key)
            .build()
            .unwrap();

        assert!(request.headers()["x-api-key"].is_sensitive());
    }
}
