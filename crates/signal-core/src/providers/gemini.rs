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
};

const OFFICIAL_GEMINI_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
const GEMINI_CLIENT_HEADER: &str = "ai-daily-signal/0.1.0";

pub struct GeminiProvider {
    client: Client,
    base: String,
    sleeper: TokioRetrySleeper,
}

impl GeminiProvider {
    pub fn official() -> Result<Self, ProviderFailure> {
        Self::with_base(OFFICIAL_GEMINI_BASE)
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
        Self::with_base(&format!("{}/v1beta", origin.as_str().trim_end_matches('/')))
    }

    fn with_base(base: &str) -> Result<Self, ProviderFailure> {
        let base = base.trim_end_matches('/');
        reqwest::Url::parse(base).map_err(|_| not_sent(ProviderFailureKind::Transport))?;
        Ok(Self {
            client: shared_http_client()?,
            base: base.to_owned(),
            sleeper: TokioRetrySleeper,
        })
    }

    fn generate_content_endpoint(
        &self,
        request: &ProviderRequest,
    ) -> Result<String, ProviderFailure> {
        let model = normalized_model_for_route(&request.model)?;
        Ok(format!(
            "{}/models/{}:generateContent",
            self.base,
            encode_path_segment(model)
        ))
    }

    async fn send_attempt(
        &self,
        request: &ProviderRequest,
        credential: &ResolvedCredential,
    ) -> Result<ProviderResponse, RetryAttemptFailure> {
        let settings = SummarySettings::default();
        let body = json!({
            "systemInstruction": {
                "parts": [{"text": request.system_text}],
            },
            "contents": [{
                "role": "user",
                "parts": [{"text": request.user_text}],
            }],
            "generationConfig": {
                "maxOutputTokens": request.max_output_tokens,
                "responseMimeType": "application/json",
                "responseSchema": {
                    "type": "OBJECT",
                    "properties": {
                        "what_happened": {
                            "type": "STRING",
                            "maxLength": settings.what_happened_max_chars.to_string(),
                        },
                        "why_it_matters": {
                            "type": "STRING",
                            "maxLength": settings.why_it_matters_max_chars.to_string(),
                        },
                        "caveat": {
                            "type": "STRING",
                            "nullable": true,
                            "maxLength": settings.caveat_max_chars.to_string(),
                        },
                    },
                    "required": ["what_happened", "why_it_matters", "caveat"],
                },
            },
        });
        let endpoint = self
            .generate_content_endpoint(request)
            .map_err(|failure| RetryAttemptFailure::new(failure, None))?;
        let mut api_key = HeaderValue::from_str(credential.expose_secret()).map_err(|_| {
            RetryAttemptFailure::new(not_sent(ProviderFailureKind::Transport), None)
        })?;
        api_key.set_sensitive(true);
        let response = self
            .client
            .post(endpoint)
            .header("x-goog-api-key", api_key)
            .header("x-goog-api-client", GEMINI_CLIENT_HEADER)
            .timeout(request.timeout)
            .json(&body)
            .send()
            .await
            .map_err(transport_attempt_failure)?;
        parse_http_response(response).await
    }
}

#[async_trait::async_trait]
impl SummaryProvider for GeminiProvider {
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
struct GenerateContentEnvelope {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "promptFeedback")]
    prompt_feedback: Option<PromptFeedback>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Deserialize)]
struct PromptFeedback {
    #[serde(rename = "blockReason")]
    block_reason: Option<String>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize)]
struct GeminiPart {
    text: Option<String>,
    #[serde(default)]
    thought: bool,
}

#[derive(Deserialize)]
struct UsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u64>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u64>,
}

async fn parse_http_response(response: Response) -> Result<ProviderResponse, RetryAttemptFailure> {
    if !response.status().is_success() {
        return Err(http_status_attempt_failure(&response));
    }

    let envelope: GenerateContentEnvelope = read_json_response(response)
        .await
        .map_err(|failure| RetryAttemptFailure::new(failure, None))?;
    if envelope
        .prompt_feedback
        .is_some_and(|feedback| feedback.block_reason.is_some())
    {
        return Err(malformed_attempt());
    }
    let mut selected_text: Option<String> = None;
    for candidate in envelope.candidates {
        if candidate.finish_reason.as_deref() != Some("STOP") {
            return Err(malformed_attempt());
        }
        let text = candidate
            .content
            .into_iter()
            .flat_map(|content| content.parts)
            .filter(|part| !part.thought)
            .filter_map(|part| part.text)
            .collect::<String>();
        if text.trim().is_empty() {
            continue;
        }
        if selected_text
            .as_ref()
            .is_some_and(|selected| selected != &text)
        {
            return Err(malformed_attempt());
        }
        selected_text = Some(text);
    }
    let text = selected_text.ok_or_else(malformed_attempt)?;
    let fields = parse_ai_summary(&text, &SummarySettings::default())
        .map_err(|failure| RetryAttemptFailure::new(failure, None))?;
    let usage = envelope.usage_metadata.and_then(|usage| {
        Some(ProviderUsage {
            input_tokens: usage.prompt_token_count?,
            output_tokens: usage.candidates_token_count?,
        })
    });
    Ok(ProviderResponse { fields, usage })
}

fn normalized_model_for_route(model: &str) -> Result<&str, ProviderFailure> {
    let model = model.strip_prefix("models/").unwrap_or(model);
    if model.is_empty() || model.chars().any(char::is_control) {
        return Err(not_sent(ProviderFailureKind::ProviderRejected));
    }
    Ok(model)
}

pub(super) fn valid_profile_model(model: &str) -> bool {
    normalized_model_for_route(model.trim()).is_ok()
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
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

fn transport_attempt_failure(error: reqwest::Error) -> RetryAttemptFailure {
    let (kind, charge_status) = if error.is_timeout() {
        (
            ProviderFailureKind::Timeout,
            RequestChargeStatus::PossiblySent,
        )
    } else if error.is_connect() {
        (ProviderFailureKind::Transport, RequestChargeStatus::NotSent)
    } else {
        (
            ProviderFailureKind::Transport,
            RequestChargeStatus::PossiblySent,
        )
    };
    RetryAttemptFailure::new(ProviderFailure::new(kind, charge_status), None)
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
