use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use signal_core::{
    AnthropicProvider, ProviderFailureKind, ProviderRequest, ProviderUsage, RequestChargeStatus,
    ResolvedCredential, SummaryProvider,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const SENTINEL_SECRET: &str = "SENTINEL-ANTHROPIC-SECRET";

fn request_with_retries(max_retries: u32) -> ProviderRequest {
    request_with_limits(max_retries, 30)
}

fn request_with_limits(max_retries: u32, timeout_seconds: u64) -> ProviderRequest {
    let mut profile = signal_core::test_support::model_profile(
        "official-anthropic",
        signal_core::ProviderKind::Anthropic,
    );
    profile.model = "opaque/model:2026-08-29".to_owned();
    profile.limits.max_output_tokens = 321;
    profile.limits.max_retries = max_retries;
    profile.limits.timeout_seconds = timeout_seconds;
    let prompt = signal_core::AiSummaryPrompt {
        system_text: "Return the required summary JSON.".to_owned(),
        user_text: r#"{"normalized_title":"a deterministic signal"}"#.to_owned(),
    };
    ProviderRequest::from_profile("SENTINEL-STORY-ID", &profile, prompt).unwrap()
}

fn valid_message(text: &str) -> Value {
    json!({
        "id": "msg_fixture",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": text}],
        "usage": {"input_tokens": 41, "output_tokens": 17}
    })
}

#[tokio::test]
async fn official_messages_maps_the_wire_contract_and_concatenates_text_blocks() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg_fixture",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "unused", "name": "unused", "input": {}},
                {"type": "text", "text": "{\"what_happened\":\"A deterministic event happened.\","},
                {"type": "thinking", "thinking": "unused"},
                {"type": "text", "text": "\"why_it_matters\":\"It has a deterministic consequence.\",\"caveat\":null}"}
            ],
            "usage": {"input_tokens": 18446744073709551615u64, "output_tokens": 17}
        })))
        .mount(&server)
        .await;

    let response = AnthropicProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &request_with_retries(1),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap();

    assert_eq!(
        response.fields.what_happened,
        "A deterministic event happened."
    );
    assert_eq!(
        response.fields.why_it_matters,
        "It has a deterministic consequence."
    );
    assert_eq!(response.fields.caveat, None);
    assert_eq!(
        response.usage,
        Some(ProviderUsage {
            input_tokens: u64::MAX,
            output_tokens: 17,
        })
    );

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(
        request.headers.get("x-api-key").unwrap().to_str().unwrap(),
        SENTINEL_SECRET
    );
    assert_eq!(
        request
            .headers
            .get("anthropic-version")
            .unwrap()
            .to_str()
            .unwrap(),
        "2023-06-01"
    );
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(
        body,
        json!({
            "model": "opaque/model:2026-08-29",
            "system": "Return the required summary JSON.",
            "messages": [{
                "role": "user",
                "content": r#"{"normalized_title":"a deterministic signal"}"#
            }],
            "max_tokens": 321,
            "stream": false
        })
    );
    assert_sentinel_only_in_x_api_key(request);
}

#[test]
fn official_test_origin_rejects_external_and_nonroot_origins_before_any_request() {
    for origin in [
        "https://SENTINEL-EXTERNAL.example",
        "https://localhost/not-an-origin",
        "https://credential@localhost/",
    ] {
        let failure = AnthropicProvider::official_for_test(origin)
            .err()
            .expect("unsafe test origins must be rejected");
        assert_eq!(failure.kind(), ProviderFailureKind::Transport);
        assert_eq!(failure.charge_status(), RequestChargeStatus::NotSent);
        assert!(!format!("{failure:?} {failure}").contains("SENTINEL"));
    }
}

#[test]
fn provider_request_revalidates_a_mutated_invalid_anthropic_profile_before_credentials() {
    let mut profile = signal_core::test_support::model_profile(
        "invalid-anthropic",
        signal_core::ProviderKind::Anthropic,
    );
    profile.endpoint = Some("https://SENTINEL-EXTERNAL.example/v1".parse().unwrap());

    let error = ProviderRequest::from_profile(
        "story",
        &profile,
        signal_core::AiSummaryPrompt {
            system_text: "system".to_owned(),
            user_text: "user".to_owned(),
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        signal_core::SignalError::InvalidConfiguration(_)
    ));
    assert!(!format!("{error:?} {error}").contains("SENTINEL"));
}

#[tokio::test]
async fn authentication_failure_is_not_retried_and_never_exposes_the_provider_body() {
    const SENTINEL_ERROR_BODY: &str = "SENTINEL-ANTHROPIC-ERROR-BODY";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_string(SENTINEL_ERROR_BODY))
        .mount(&server)
        .await;

    let failure = AnthropicProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &request_with_retries(3),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::Authentication);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    let rendered = format!("{failure:?} {failure}");
    assert!(!rendered.contains(SENTINEL_ERROR_BODY));
    assert!(!rendered.contains(SENTINEL_SECRET));
}

#[derive(Clone)]
struct TransientThenSuccess {
    status: u16,
    attempts: Arc<AtomicUsize>,
}

impl Respond for TransientThenSuccess {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            ResponseTemplate::new(self.status)
                .insert_header("Retry-After", "0")
                .set_body_string("SENTINEL-TRANSIENT-ERROR-BODY")
        } else {
            ResponseTemplate::new(200).set_body_json(valid_message(
                r#"{"what_happened":"The retry succeeded.","why_it_matters":"Only one retry was used.","caveat":null}"#,
            ))
        }
    }
}

#[tokio::test]
async fn rate_limits_and_server_errors_use_one_shared_retry() {
    for status in [429, 503] {
        let server = MockServer::start().await;
        let attempts = Arc::new(AtomicUsize::new(0));
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(TransientThenSuccess {
                status,
                attempts: Arc::clone(&attempts),
            })
            .mount(&server)
            .await;

        let response = AnthropicProvider::official_for_test(server.uri())
            .unwrap()
            .generate(
                &request_with_retries(1),
                &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
            )
            .await
            .unwrap();

        assert_eq!(response.fields.what_happened, "The retry succeeded.");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        for request in &requests {
            assert_sentinel_only_in_x_api_key(request);
        }
    }
}

#[derive(Clone)]
struct TimeoutThenSuccess {
    attempts: Arc<AtomicUsize>,
}

impl Respond for TimeoutThenSuccess {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            ResponseTemplate::new(200).set_delay(Duration::from_secs(2))
        } else {
            ResponseTemplate::new(200).set_body_json(valid_message(
                r#"{"what_happened":"The timeout retry succeeded.","why_it_matters":"Timeouts are transient.","caveat":null}"#,
            ))
        }
    }
}

#[tokio::test]
async fn timeouts_use_one_shared_retry() {
    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(TimeoutThenSuccess {
            attempts: Arc::clone(&attempts),
        })
        .mount(&server)
        .await;

    let response = AnthropicProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &request_with_limits(1, 1),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap();

    assert_eq!(
        response.fields.what_happened,
        "The timeout retry succeeded."
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn textless_or_malformed_successes_are_rejected_without_echoing_bodies() {
    const SENTINEL_MALFORMED_BODY: &str = "SENTINEL-ANTHROPIC-MALFORMED-BODY";
    let cases = [
        ResponseTemplate::new(200).set_body_json(json!({
            "type": "message",
            "content": [{"type": "tool_use", "id": "unused", "name": "unused", "input": {}}]
        })),
        ResponseTemplate::new(200).set_body_string(SENTINEL_MALFORMED_BODY),
    ];

    for response_template in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(response_template)
            .mount(&server)
            .await;

        let failure = AnthropicProvider::official_for_test(server.uri())
            .unwrap()
            .generate(
                &request_with_retries(1),
                &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
            )
            .await
            .unwrap_err();
        assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
        assert!(!format!("{failure:?} {failure}").contains(SENTINEL_MALFORMED_BODY));
    }
}

#[tokio::test]
async fn oversized_success_body_is_rejected_by_the_shared_cap_without_echoing_it() {
    const SENTINEL_OVERSIZED_BODY: &str = "SENTINEL-ANTHROPIC-OVERSIZED-BODY";
    let server = MockServer::start().await;
    let body = format!("{SENTINEL_OVERSIZED_BODY}{}", "x".repeat(256 * 1024 + 1));
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let failure = AnthropicProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &request_with_retries(1),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
    assert!(!format!("{failure:?} {failure}").contains(SENTINEL_OVERSIZED_BODY));
}

fn assert_sentinel_only_in_x_api_key(request: &wiremock::Request) {
    assert!(!request.url.as_str().contains(SENTINEL_SECRET));
    assert!(!String::from_utf8_lossy(&request.body).contains(SENTINEL_SECRET));
    assert!(request.headers.get("authorization").is_none());
    for (name, value) in &request.headers {
        if name.as_str() != "x-api-key" {
            assert!(!value.to_str().unwrap_or_default().contains(SENTINEL_SECRET));
        }
    }
}
