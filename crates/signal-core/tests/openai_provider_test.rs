use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use secrecy::SecretString;
use serde_json::{Value, json};
use signal_core::{
    AiGenerationCoordinator, ApiDialect, CredentialStore, NewModelProfile, OpenAiProvider,
    ProviderFailureKind, ProviderRegistry, ProviderRequest, ProviderUsage, RequestChargeStatus,
    ResolvedCredential, SummaryProvider, SummarySettings,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const SENTINEL_SECRET: &str = "SENTINEL-OPENAI-SECRET";

fn official_request() -> ProviderRequest {
    official_request_with_retries(1)
}

fn official_request_with_retries(max_retries: u32) -> ProviderRequest {
    let mut profile = signal_core::test_support::model_profile(
        "official-openai",
        signal_core::ProviderKind::OpenAi,
    );
    profile.model = "  opaque/model:2026-08-29  ".to_owned();
    profile.limits.max_output_tokens = 321;
    profile.limits.max_retries = max_retries;
    let prompt = signal_core::AiSummaryPrompt {
        system_text: "Return the required summary JSON.".to_owned(),
        user_text: r#"{"normalized_title":"a deterministic signal"}"#.to_owned(),
    };
    ProviderRequest::from_profile("story-1", &profile, prompt).unwrap()
}

fn compatible_request(endpoint: &str, dialect: ApiDialect) -> ProviderRequest {
    let mut profile = signal_core::test_support::model_profile(
        "custom-openai",
        signal_core::ProviderKind::OpenAiCompatible,
    );
    profile.model = "vendor/opaque:model".to_owned();
    profile.endpoint = Some(endpoint.parse().unwrap());
    profile.dialect = Some(dialect);
    profile.limits.max_output_tokens = 222;
    profile.limits.max_retries = 1;
    let prompt = signal_core::AiSummaryPrompt {
        system_text: "System summary instructions.".to_owned(),
        user_text: "User story JSON.".to_owned(),
    };
    ProviderRequest::from_profile("story-custom", &profile, prompt).unwrap()
}

#[tokio::test]
async fn official_responses_maps_the_wire_contract_and_collects_all_output_text() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "resp_fixture",
            "status": "completed",
            "output": [
                {"type": "reasoning", "summary": []},
                {
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "{\"what_happened\":\"A deterministic event happened.\","
                    }]
                },
                {
                    "type": "message",
                    "content": [
                        {"type": "refusal", "refusal": "unused"},
                        {
                            "type": "output_text",
                            "text": "\"why_it_matters\":\"It has a deterministic consequence.\",\"caveat\":null}"
                        }
                    ]
                }
            ],
            "usage": {"input_tokens": 41, "output_tokens": 17, "total_tokens": 58}
        })))
        .mount(&server)
        .await;

    let provider = OpenAiProvider::official_for_test(server.uri()).unwrap();
    let response = provider
        .generate(
            &official_request(),
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
            input_tokens: 41,
            output_tokens: 17,
        })
    );

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(
        request
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("Bearer {SENTINEL_SECRET}")
    );
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], "  opaque/model:2026-08-29  ");
    assert_eq!(body["instructions"], "Return the required summary JSON.");
    assert_eq!(
        body["input"],
        r#"{"normalized_title":"a deterministic signal"}"#
    );
    assert_eq!(
        body["text"]["format"],
        json!({
            "type": "json_schema",
            "name": "ai_summary",
            "strict": true,
            "schema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "what_happened": {
                        "type": "string",
                        "maxLength": 600
                    },
                    "why_it_matters": {
                        "type": "string",
                        "maxLength": 600
                    },
                    "caveat": {
                        "type": ["string", "null"],
                        "maxLength": 300
                    }
                },
                "required": ["what_happened", "why_it_matters", "caveat"]
            }
        })
    );
    assert_eq!(body["max_output_tokens"], 321);
    assert_eq!(body["store"], false);
    assert!(body.get("response_format").is_none());
    assert_sentinel_only_in_authorization(request);
}

#[tokio::test]
async fn persisted_opaque_model_reaches_cache_report_and_provider_unchanged() {
    const OPAQUE_MODEL: &str = "  vendor/opaque:model\t";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "{\"what_happened\":\"The opaque model round trip completed.\",\"why_it_matters\":\"Identity bytes stayed stable.\",\"caveat\":null}"
                }]
            }]
        })))
        .mount(&server)
        .await;

    let store = signal_core::test_support::temporary_store();
    let fixture = signal_core::test_support::model_profile(
        "persisted-opaque",
        signal_core::ProviderKind::OpenAi,
    );
    let profile = NewModelProfile {
        name: fixture.name,
        provider: fixture.provider,
        model: OPAQUE_MODEL.to_owned(),
        endpoint: fixture.endpoint,
        dialect: fixture.dialect,
        credential: fixture.credential,
        consented_at: fixture.consented_at,
        enabled: fixture.enabled,
        limits: fixture.limits,
    }
    .into_model_profile(fixture.id, fixture.created_at, fixture.updated_at)
    .unwrap();
    assert_eq!(profile.model, OPAQUE_MODEL);
    store.create_model_profile(&profile).unwrap();
    let persisted = store.find_model_profile(profile.id).unwrap().unwrap();
    assert_eq!(persisted.model, OPAQUE_MODEL);

    let story = signal_core::test_support::story_fixture("story-1");
    let settings = SummarySettings::default();
    let expected_cache_key = signal_core::summary_cache_key(
        &story,
        &persisted,
        signal_core::AI_SUMMARY_PROMPT_VERSION,
        &settings,
    )
    .unwrap();
    let mut trimmed = persisted.clone();
    trimmed.model = OPAQUE_MODEL.trim().to_owned();
    assert_ne!(
        expected_cache_key,
        signal_core::summary_cache_key(
            &story,
            &trimmed,
            signal_core::AI_SUMMARY_PROMPT_VERSION,
            &settings,
        )
        .unwrap()
    );

    let credential_store = signal_core::test_support::MemoryCredentialStore::default();
    credential_store
        .set(
            &persisted.credential,
            SecretString::from(SENTINEL_SECRET.to_owned()),
        )
        .unwrap();
    let environment = signal_core::test_support::MemoryEnvironmentReader::default();
    let mut providers = ProviderRegistry::new();
    providers.register(
        signal_core::ProviderKind::OpenAi,
        Arc::new(OpenAiProvider::official_for_test(server.uri()).unwrap()),
    );
    let report = AiGenerationCoordinator::new(&store, &credential_store, &environment, &providers)
        .summarize(
            &story,
            &persisted,
            true,
            signal_core::test_support::fixed_now(),
        )
        .await
        .unwrap();
    let summary = report.summary.unwrap();
    assert_eq!(summary.model, OPAQUE_MODEL);
    assert_eq!(summary.cache_key, expected_cache_key);
    assert_eq!(
        store.list_summary_variants(&story.id).unwrap()[0].model,
        OPAQUE_MODEL
    );

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], OPAQUE_MODEL);
    assert_sentinel_only_in_authorization(&requests[0]);
}

#[test]
fn official_test_origin_accepts_only_numeric_loopback_root_origins_before_any_request() {
    for origin in ["https://SENTINEL-EXTERNAL.example", "http://localhost"] {
        let failure = OpenAiProvider::official_for_test(origin)
            .err()
            .expect("unsafe test origins must be rejected");

        assert_eq!(failure.kind(), ProviderFailureKind::Transport);
        assert_eq!(failure.charge_status(), RequestChargeStatus::NotSent);
        assert!(!format!("{failure:?} {failure}").contains("SENTINEL"));
    }

    for origin in ["http://127.0.0.1:8181", "http://[::1]:8181"] {
        assert!(OpenAiProvider::official_for_test(origin).is_ok());
    }
}

#[tokio::test]
async fn custom_responses_preserves_the_base_path_and_normalizes_trailing_slashes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gateway/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "{\"what_happened\":\"Custom Responses worked.\",\"why_it_matters\":\"The base path was preserved.\",\"caveat\":null}"
                }]
            }]
        })))
        .mount(&server)
        .await;

    let base = format!("{}/gateway/v1///", server.uri());
    let response = OpenAiProvider::compatible()
        .unwrap()
        .generate(
            &compatible_request(&base, ApiDialect::Responses),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap();

    assert_eq!(response.fields.what_happened, "Custom Responses worked.");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "vendor/opaque:model");
    assert_sentinel_only_in_authorization(&requests[0]);
}

#[tokio::test]
async fn custom_chat_completions_maps_messages_json_mode_choice_and_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_fixture",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "{\"what_happened\":\"Chat Completions worked.\",\"why_it_matters\":\"The dialect mapping is explicit.\",\"caveat\":\"Compatibility varies.\"}"
                },
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 51, "completion_tokens": 19, "total_tokens": 70}
        })))
        .mount(&server)
        .await;

    let base = format!("{}/api/v1/", server.uri());
    let response = OpenAiProvider::compatible()
        .unwrap()
        .generate(
            &compatible_request(&base, ApiDialect::ChatCompletions),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap();

    assert_eq!(response.fields.what_happened, "Chat Completions worked.");
    assert_eq!(
        response.fields.caveat.as_deref(),
        Some("Compatibility varies.")
    );
    assert_eq!(
        response.usage,
        Some(ProviderUsage {
            input_tokens: 51,
            output_tokens: 19,
        })
    );

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "vendor/opaque:model");
    assert_eq!(body["max_tokens"], 222);
    assert_eq!(body["response_format"], json!({"type": "json_object"}));
    assert_eq!(
        body["messages"],
        json!([
            {"role": "system", "content": "System summary instructions."},
            {"role": "user", "content": "User story JSON."}
        ])
    );
    assert!(body.get("text").is_none());
    assert_sentinel_only_in_authorization(&requests[0]);
}

#[tokio::test]
async fn redirects_are_refused_without_forwarding_the_credential() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/credential-catcher"))
        .mount(&server)
        .await;

    let failure = OpenAiProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &official_request_with_retries(3),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::ProviderRejected);
    assert_eq!(failure.charge_status(), RequestChargeStatus::PossiblySent);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/v1/responses");
}

#[tokio::test]
async fn authentication_failure_is_not_retried_and_redacts_the_provider_body() {
    const SENTINEL_ERROR_BODY: &str = "SENTINEL-PROVIDER-ERROR-BODY";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(401).set_body_string(SENTINEL_ERROR_BODY))
        .mount(&server)
        .await;

    let failure = OpenAiProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &official_request_with_retries(3),
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

#[tokio::test]
async fn connection_failure_is_uncharged_and_not_retried() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);

    let failure = OpenAiProvider::official_for_test(origin)
        .unwrap()
        .generate(
            &official_request_with_retries(3),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::Transport);
    assert_eq!(failure.charge_status(), RequestChargeStatus::NotSent);
}

#[tokio::test]
async fn malformed_bearer_credential_is_a_not_sent_builder_failure() {
    let server = MockServer::start().await;
    let failure = OpenAiProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &official_request_with_retries(3),
            &ResolvedCredential::new("invalid\ncredential".to_owned()),
        )
        .await
        .unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::Transport);
    assert_eq!(failure.charge_status(), RequestChargeStatus::NotSent);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[derive(Clone)]
struct RateLimitThenSuccess {
    attempts: Arc<AtomicUsize>,
}

impl Respond for RateLimitThenSuccess {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "0")
                .set_body_string("SENTINEL-RATE-LIMIT-BODY")
        } else {
            ResponseTemplate::new(200).set_body_json(json!({
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "{\"what_happened\":\"The retry succeeded.\",\"why_it_matters\":\"Only one retry was used.\",\"caveat\":null}"
                    }]
                }]
            }))
        }
    }
}

#[tokio::test]
async fn rate_limit_then_success_uses_exactly_one_retry() {
    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(RateLimitThenSuccess {
            attempts: Arc::clone(&attempts),
        })
        .mount(&server)
        .await;

    let response = OpenAiProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &official_request_with_retries(1),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap();

    assert_eq!(response.fields.what_happened, "The retry succeeded.");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn malformed_success_body_is_redacted() {
    const SENTINEL_MALFORMED_BODY: &str = "SENTINEL-MALFORMED-BODY";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SENTINEL_MALFORMED_BODY))
        .mount(&server)
        .await;

    let failure = OpenAiProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &official_request(),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
    assert!(!format!("{failure:?} {failure}").contains(SENTINEL_MALFORMED_BODY));
}

#[tokio::test]
async fn oversized_success_body_is_rejected_by_the_shared_cap_and_redacted() {
    const SENTINEL_OVERSIZED_BODY: &str = "SENTINEL-OVERSIZED-BODY";
    let server = MockServer::start().await;
    let body = format!("{SENTINEL_OVERSIZED_BODY}{}", "x".repeat(256 * 1024 + 1));
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let failure = OpenAiProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &official_request(),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
    assert!(!format!("{failure:?} {failure}").contains(SENTINEL_OVERSIZED_BODY));
}

#[tokio::test]
async fn noncompleted_or_textless_responses_are_malformed() {
    for body in [
        json!({"status": "incomplete", "output": []}),
        json!({"status": "failed", "output": []}),
        json!({"status": "completed", "output": [{"type": "reasoning"}]}),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let failure = OpenAiProvider::official_for_test(server.uri())
            .unwrap()
            .generate(
                &official_request(),
                &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
            )
            .await
            .unwrap_err();
        assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
    }
}

#[tokio::test]
async fn chat_completions_requires_exactly_one_nonempty_assistant_text() {
    for choices in [
        json!([]),
        json!([{"message": {"role": "assistant", "content": "  "}}]),
        json!([
            {
                "message": {
                    "role": "assistant",
                    "content": "{\"what_happened\":\"First.\",\"why_it_matters\":\"First reason.\",\"caveat\":null}"
                }
            },
            {
                "message": {
                    "role": "assistant",
                    "content": "{\"what_happened\":\"Second.\",\"why_it_matters\":\"Second reason.\",\"caveat\":null}"
                }
            }
        ]),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": choices
            })))
            .mount(&server)
            .await;

        let failure = OpenAiProvider::compatible()
            .unwrap()
            .generate(
                &compatible_request(&format!("{}/v1", server.uri()), ApiDialect::ChatCompletions),
                &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
            )
            .await
            .unwrap_err();
        assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
    }
}

fn assert_sentinel_only_in_authorization(request: &wiremock::Request) {
    assert!(!request.url.as_str().contains(SENTINEL_SECRET));
    assert!(!String::from_utf8_lossy(&request.body).contains(SENTINEL_SECRET));
    for (name, value) in &request.headers {
        if name.as_str() != "authorization" {
            assert!(!value.to_str().unwrap_or_default().contains(SENTINEL_SECRET));
        }
    }
}
