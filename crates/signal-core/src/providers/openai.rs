use std::time::SystemTime;

use reqwest::{Client, Response};
use serde::Deserialize;
use serde_json::json;

use crate::{ApiDialect, ResolvedCredential, SummarySettings};

use super::{
    ProviderFailure, ProviderFailureKind, ProviderRequest, ProviderResponse, ProviderUsage,
    RequestChargeStatus, RetryAttemptFailure, RetryPolicy, SummaryProvider, TokioRetrySleeper,
    parse_ai_summary, read_json_response, retry_provider_operation, shared_http_client,
};

const OFFICIAL_OPENAI_ORIGIN: &str = "https://api.openai.com";

pub struct OpenAiProvider {
    client: Client,
    mode: OpenAiMode,
    sleeper: TokioRetrySleeper,
}

enum OpenAiMode {
    Official { origin: String },
    Compatible,
}

impl OpenAiProvider {
    pub fn official() -> Result<Self, ProviderFailure> {
        Self::with_official_origin(OFFICIAL_OPENAI_ORIGIN)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn official_for_test(origin: impl AsRef<str>) -> Result<Self, ProviderFailure> {
        let origin = reqwest::Url::parse(origin.as_ref())
            .map_err(|_| not_sent(ProviderFailureKind::Transport))?;
        let valid_origin = matches!(origin.scheme(), "http" | "https")
            && origin
                .host_str()
                .is_some_and(crate::models::is_literal_loopback_host)
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
        let endpoint = format!("{origin}/v1/responses");
        reqwest::Url::parse(&endpoint).map_err(|_| not_sent(ProviderFailureKind::Transport))?;
        Ok(Self {
            client: shared_http_client()?,
            mode: OpenAiMode::Official {
                origin: origin.to_owned(),
            },
            sleeper: TokioRetrySleeper,
        })
    }

    pub fn compatible() -> Result<Self, ProviderFailure> {
        Ok(Self {
            client: shared_http_client()?,
            mode: OpenAiMode::Compatible,
            sleeper: TokioRetrySleeper,
        })
    }

    fn responses_endpoint(&self, request: &ProviderRequest) -> Result<String, ProviderFailure> {
        match &self.mode {
            OpenAiMode::Official { origin } => Ok(format!("{origin}/v1/responses")),
            OpenAiMode::Compatible => {
                if request.dialect != Some(ApiDialect::Responses) {
                    return Err(not_sent(ProviderFailureKind::ProviderRejected));
                }
                append_endpoint_path(request, "/responses")
            }
        }
    }

    async fn send_responses(
        &self,
        request: &ProviderRequest,
        credential: &ResolvedCredential,
    ) -> Result<ProviderResponse, RetryAttemptFailure> {
        let settings = SummarySettings::default();
        let body = json!({
            "model": request.model,
            "instructions": request.system_text,
            "input": request.user_text,
            "max_output_tokens": request.max_output_tokens,
            "store": false,
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "ai_summary",
                    "strict": true,
                    "schema": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "what_happened": {
                                "type": "string",
                                "maxLength": settings.what_happened_max_chars,
                            },
                            "why_it_matters": {
                                "type": "string",
                                "maxLength": settings.why_it_matters_max_chars,
                            },
                            "caveat": {
                                "type": ["string", "null"],
                                "maxLength": settings.caveat_max_chars,
                            },
                        },
                        "required": ["what_happened", "why_it_matters", "caveat"],
                    },
                },
            },
        });
        let endpoint = self
            .responses_endpoint(request)
            .map_err(|failure| RetryAttemptFailure::new(failure, None))?;
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(credential.expose_secret())
            .timeout(request.timeout)
            .json(&body)
            .send()
            .await
            .map_err(transport_attempt_failure)?;
        parse_http_response(response).await
    }

    async fn send_chat_completions(
        &self,
        request: &ProviderRequest,
        credential: &ResolvedCredential,
    ) -> Result<ProviderResponse, RetryAttemptFailure> {
        let endpoint = append_endpoint_path(request, "/chat/completions")
            .map_err(|failure| RetryAttemptFailure::new(failure, None))?;
        let body = json!({
            "model": request.model,
            "messages": [
                {"role": "system", "content": request.system_text},
                {"role": "user", "content": request.user_text},
            ],
            "max_tokens": request.max_output_tokens,
            "response_format": {"type": "json_object"},
        });
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(credential.expose_secret())
            .timeout(request.timeout)
            .json(&body)
            .send()
            .await
            .map_err(transport_attempt_failure)?;
        parse_chat_http_response(response).await
    }

    async fn send_attempt(
        &self,
        request: &ProviderRequest,
        credential: &ResolvedCredential,
    ) -> Result<ProviderResponse, RetryAttemptFailure> {
        match (&self.mode, request.dialect) {
            (OpenAiMode::Official { .. }, _) | (_, Some(ApiDialect::Responses)) => {
                self.send_responses(request, credential).await
            }
            (OpenAiMode::Compatible, Some(ApiDialect::ChatCompletions)) => {
                self.send_chat_completions(request, credential).await
            }
            (OpenAiMode::Compatible, None) => Err(RetryAttemptFailure::new(
                not_sent(ProviderFailureKind::ProviderRejected),
                None,
            )),
        }
    }
}

#[async_trait::async_trait]
impl SummaryProvider for OpenAiProvider {
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
struct ResponsesEnvelope {
    status: String,
    #[serde(default)]
    output: Vec<ResponsesOutputItem>,
    usage: Option<ResponsesUsage>,
}

#[derive(Deserialize)]
struct ResponsesOutputItem {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    content: Vec<ResponsesContentItem>,
}

#[derive(Deserialize)]
struct ResponsesContentItem {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ResponsesUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct ChatCompletionsEnvelope {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    role: String,
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

async fn parse_http_response(response: Response) -> Result<ProviderResponse, RetryAttemptFailure> {
    if !response.status().is_success() {
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| RetryPolicy::parse_retry_after(value, SystemTime::now()));
        return Err(RetryAttemptFailure::new(
            sent(ProviderFailureKind::from_http_status(status)),
            retry_after,
        ));
    }

    let envelope: ResponsesEnvelope = read_json_response(response)
        .await
        .map_err(|failure| RetryAttemptFailure::new(failure, None))?;
    if envelope.status != "completed" {
        return Err(malformed_attempt());
    }

    let text = envelope
        .output
        .iter()
        .filter(|item| item.kind == "message")
        .flat_map(|item| &item.content)
        .filter(|item| item.kind == "output_text")
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

async fn parse_chat_http_response(
    response: Response,
) -> Result<ProviderResponse, RetryAttemptFailure> {
    if !response.status().is_success() {
        return Err(http_status_attempt_failure(&response));
    }
    let envelope: ChatCompletionsEnvelope = read_json_response(response)
        .await
        .map_err(|failure| RetryAttemptFailure::new(failure, None))?;
    let mut texts = envelope
        .choices
        .into_iter()
        .filter(|choice| choice.message.role == "assistant")
        .filter_map(|choice| choice.message.content)
        .filter(|content| !content.trim().is_empty());
    let Some(text) = texts.next() else {
        return Err(malformed_attempt());
    };
    if texts.next().is_some() {
        return Err(malformed_attempt());
    }
    let fields = parse_ai_summary(&text, &SummarySettings::default())
        .map_err(|failure| RetryAttemptFailure::new(failure, None))?;
    let usage = envelope.usage.and_then(|usage| {
        Some(ProviderUsage {
            input_tokens: usage.prompt_tokens?,
            output_tokens: usage.completion_tokens?,
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

fn append_endpoint_path(
    request: &ProviderRequest,
    suffix: &str,
) -> Result<String, ProviderFailure> {
    let mut endpoint = request
        .endpoint
        .clone()
        .ok_or_else(|| not_sent(ProviderFailureKind::ProviderRejected))?;
    let base_path = endpoint.path().trim_end_matches('/');
    endpoint.set_path(&format!("{base_path}{suffix}"));
    Ok(endpoint.into())
}
