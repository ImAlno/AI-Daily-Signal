use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use serde_json::{Value, json};
use signal_core::{
    GeminiProvider, ProviderFailureKind, ProviderRequest, ProviderUsage, RequestChargeStatus,
    ResolvedCredential, SummaryProvider,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const SENTINEL_SECRET: &str = "SENTINEL-GEMINI-SECRET";

fn shared_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().unwrap())
}

macro_rules! shared_runtime_test {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() {
            shared_runtime().block_on(async $body);
        }
    };
}

fn request_for_model(model: &str) -> ProviderRequest {
    request_for_model_with_limits(model, 1, 30)
}

fn request_for_model_with_limits(
    model: &str,
    max_retries: u32,
    timeout_seconds: u64,
) -> ProviderRequest {
    let mut profile = signal_core::test_support::model_profile(
        "official-gemini",
        signal_core::ProviderKind::Gemini,
    );
    profile.model = model.to_owned();
    profile.limits.max_output_tokens = 321;
    profile.limits.max_retries = max_retries;
    profile.limits.timeout_seconds = timeout_seconds;
    let prompt = signal_core::AiSummaryPrompt {
        system_text: "Return the required summary JSON.".to_owned(),
        user_text: r#"{"normalized_title":"a deterministic signal"}"#.to_owned(),
    };
    ProviderRequest::from_profile("SENTINEL-STORY-ID", &profile, prompt).unwrap()
}

fn valid_response(text: &str) -> Value {
    json!({
        "candidates": [{
            "content": {"role": "model", "parts": [{"text": text}]},
            "finishReason": "STOP"
        }]
    })
}

shared_runtime_test!(
    generate_content_maps_the_wire_contract_and_parses_text_and_u64_usage,
    {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [
                        {"thought": true, "text": "ignored thought"},
                        {"text": "{\"what_happened\":\"A deterministic event happened.\","},
                        {"inlineData": {"mimeType": "text/plain", "data": "unused"}},
                        {"text": "\"why_it_matters\":\"It has a deterministic consequence.\",\"caveat\":null}"}
                    ]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 18446744073709551615u64,
                "candidatesTokenCount": 18446744073709551615u64,
                "totalTokenCount": 1
            }
        })))
        .mount(&server)
        .await;

        let response = GeminiProvider::official_for_test(server.uri())
            .unwrap()
            .generate(
                &request_for_model("models/gemini-2.5-flash"),
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
                output_tokens: u64::MAX,
            })
        );

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.url.query(), None);
        assert_eq!(
            request
                .headers
                .get("x-goog-api-key")
                .unwrap()
                .to_str()
                .unwrap(),
            SENTINEL_SECRET
        );
        assert_eq!(
            request
                .headers
                .get("x-goog-api-client")
                .unwrap()
                .to_str()
                .unwrap(),
            "ai-daily-signal/0.1.0"
        );
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(
            body,
            json!({
                "systemInstruction": {
                    "parts": [{"text": "Return the required summary JSON."}]
                },
                "contents": [{
                    "role": "user",
                    "parts": [{"text": r#"{"normalized_title":"a deterministic signal"}"#}]
                }],
                "generationConfig": {
                    "maxOutputTokens": 321,
                    "responseMimeType": "application/json",
                    "responseSchema": {
                        "type": "OBJECT",
                        "properties": {
                            "what_happened": {"type": "STRING", "maxLength": "600"},
                            "why_it_matters": {"type": "STRING", "maxLength": "600"},
                            "caveat": {"type": "STRING", "nullable": true, "maxLength": "300"}
                        },
                        "required": ["what_happened", "why_it_matters", "caveat"]
                    }
                }
            })
        );
        assert!(body.get("model").is_none());
        assert!(!String::from_utf8_lossy(&request.body).contains("SENTINEL-STORY-ID"));
        assert_sentinel_only_in_x_goog_api_key(request);
    }
);

shared_runtime_test!(
    model_routing_strips_one_optional_prefix_and_encodes_the_remainder_as_one_segment,
    {
        let cases = [
            (
                "gemini/custom",
                "/v1beta/models/gemini%2Fcustom:generateContent",
            ),
            (
                "models/family/../name?key=leak#fragment",
                "/v1beta/models/family%2F..%2Fname%3Fkey%3Dleak%23fragment:generateContent",
            ),
            (
                "models/models/gemini",
                "/v1beta/models/models%2Fgemini:generateContent",
            ),
            (
                "models/name:variant%2F",
                "/v1beta/models/name%3Avariant%252F:generateContent",
            ),
        ];

        for (model, expected_path) in cases {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
            .and(path(expected_path))
            .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(
                r#"{"what_happened":"Safe routing worked.","why_it_matters":"The model stayed in one segment.","caveat":null}"#,
            )))
            .mount(&server)
            .await;

            let response = GeminiProvider::official_for_test(server.uri())
                .unwrap()
                .generate(
                    &request_for_model(model),
                    &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
                )
                .await
                .unwrap();

            assert_eq!(response.fields.what_happened, "Safe routing worked.");
            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].url.path(), expected_path);
            assert_eq!(requests[0].url.query(), None);
        }
    }
);

#[test]
fn provider_request_rejects_invalid_mutated_gemini_profiles_before_credentials() {
    for model in ["models/", "models/gemini\nunsafe", "gemini\u{0085}unsafe"] {
        let mut profile = signal_core::test_support::model_profile(
            "invalid-gemini",
            signal_core::ProviderKind::Gemini,
        );
        profile.model = model.to_owned();

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
        assert!(!format!("{error:?} {error}").contains(model));
    }

    let mut profile = signal_core::test_support::model_profile(
        "invalid-gemini",
        signal_core::ProviderKind::Gemini,
    );
    profile.endpoint = Some("https://SENTINEL-EXTERNAL.example/v1".parse().unwrap());
    assert!(
        ProviderRequest::from_profile(
            "story",
            &profile,
            signal_core::AiSummaryPrompt {
                system_text: "system".to_owned(),
                user_text: "user".to_owned(),
            },
        )
        .is_err()
    );
}

#[test]
fn official_test_origin_accepts_only_numeric_loopback_root_origins() {
    for origin in [
        "https://SENTINEL-EXTERNAL.example",
        "http://localhost",
        "http://127.0.0.1/not-root",
        "http://127.0.0.1/?query=unsafe",
        "https://credential@127.0.0.1/",
    ] {
        let failure = GeminiProvider::official_for_test(origin)
            .err()
            .expect("unsafe test origins must be rejected");
        assert_eq!(failure.kind(), ProviderFailureKind::Transport);
        assert_eq!(failure.charge_status(), RequestChargeStatus::NotSent);
        assert!(!format!("{failure:?} {failure}").contains("SENTINEL"));
    }

    for origin in ["http://127.0.0.1:8181", "http://[::1]:8181"] {
        assert!(GeminiProvider::official_for_test(origin).is_ok());
    }
}

shared_runtime_test!(
    prompt_feedback_block_is_rejected_even_if_a_candidate_is_present,
    {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "promptFeedback": {"blockReason": "SAFETY"},
            "candidates": [{
                "content": {"parts": [{
                    "text": r#"{"what_happened":"Unsafe selection.","why_it_matters":"Blocked feedback must win.","caveat":null}"#
                }]},
                "finishReason": "STOP"
            }]
        })))
        .mount(&server)
        .await;

        let failure = GeminiProvider::official_for_test(server.uri())
            .unwrap()
            .generate(
                &request_for_model("gemini-2.5-flash"),
                &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
            )
            .await
            .unwrap_err();

        assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
    }
);

shared_runtime_test!(multiple_conflicting_candidate_texts_are_rejected, {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [
                {
                    "content": {"parts": [{
                        "text": r#"{"what_happened":"First candidate.","why_it_matters":"It is one possible answer.","caveat":null}"#
                    }]},
                    "finishReason": "STOP"
                },
                {
                    "content": {"parts": [{
                        "text": r#"{"what_happened":"Second candidate.","why_it_matters":"It conflicts with the first.","caveat":null}"#
                    }]},
                    "finishReason": "STOP"
                }
            ]
        })))
        .mount(&server)
        .await;

    let failure = GeminiProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &request_for_model("gemini-2.5-flash"),
            &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
        )
        .await
        .unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
});

shared_runtime_test!(missing_or_non_success_finish_reasons_are_rejected, {
    for finish_reason in [None, Some("MAX_TOKENS"), Some("SAFETY"), Some("OTHER")] {
        let server = MockServer::start().await;
        let mut candidate = json!({
            "content": {"parts": [{
                "text": r#"{"what_happened":"Incomplete answer.","why_it_matters":"Its finish reason is not successful.","caveat":null}"#
            }]}
        });
        if let Some(reason) = finish_reason {
            candidate["finishReason"] = json!(reason);
        }
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [candidate]
            })))
            .mount(&server)
            .await;

        let failure = GeminiProvider::official_for_test(server.uri())
            .unwrap()
            .generate(
                &request_for_model("gemini-2.5-flash"),
                &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
            )
            .await
            .unwrap_err();

        assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
    }
});

shared_runtime_test!(
    authentication_failures_are_not_retried_and_never_expose_error_bodies_or_keys,
    {
        const SENTINEL_ERROR_BODY: &str = "SENTINEL-GEMINI-ERROR-BODY";

        for status in [401, 403] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
                .respond_with(ResponseTemplate::new(status).set_body_string(SENTINEL_ERROR_BODY))
                .mount(&server)
                .await;

            let failure = GeminiProvider::official_for_test(server.uri())
                .unwrap()
                .generate(
                    &request_for_model("gemini-2.5-flash"),
                    &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
                )
                .await
                .unwrap_err();

            assert_eq!(failure.kind(), ProviderFailureKind::Authentication);
            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests.len(), 1);
            assert_sentinel_only_in_x_goog_api_key(&requests[0]);
            let rendered = format!("{failure:?} {failure}");
            assert!(!rendered.contains(SENTINEL_ERROR_BODY));
            assert!(!rendered.contains(SENTINEL_SECRET));
        }
    }
);

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
                .set_body_string("SENTINEL-TRANSIENT-GEMINI-BODY")
        } else {
            ResponseTemplate::new(200).set_body_json(valid_response(
                r#"{"what_happened":"The retry succeeded.","why_it_matters":"The request was rebuilt safely.","caveat":null}"#,
            ))
        }
    }
}

shared_runtime_test!(
    rate_limits_and_server_errors_use_one_shared_retry_and_rebuild_the_request,
    {
        for status in [429, 503] {
            let server = MockServer::start().await;
            let attempts = Arc::new(AtomicUsize::new(0));
            Mock::given(method("POST"))
                .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
                .respond_with(TransientThenSuccess {
                    status,
                    attempts: Arc::clone(&attempts),
                })
                .mount(&server)
                .await;

            let response = GeminiProvider::official_for_test(server.uri())
                .unwrap()
                .generate(
                    &request_for_model("gemini-2.5-flash"),
                    &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
                )
                .await
                .unwrap();

            assert_eq!(response.fields.what_happened, "The retry succeeded.");
            assert_eq!(attempts.load(Ordering::SeqCst), 2);
            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests.len(), 2);
            assert_eq!(requests[0].body, requests[1].body);
            for request in &requests {
                assert_sentinel_only_in_x_goog_api_key(request);
                assert_eq!(request.url.query(), None);
            }
        }
    }
);

#[derive(Clone)]
struct TimeoutThenSuccess {
    attempts: Arc<AtomicUsize>,
}

impl Respond for TimeoutThenSuccess {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(2))
        } else {
            ResponseTemplate::new(200).set_body_json(valid_response(
                r#"{"what_happened":"The timeout retry succeeded.","why_it_matters":"Timeouts use the shared retry policy.","caveat":null}"#,
            ))
        }
    }
}

shared_runtime_test!(timeouts_use_one_shared_retry, {
    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
        .respond_with(TimeoutThenSuccess {
            attempts: Arc::clone(&attempts),
        })
        .mount(&server)
        .await;

    let response = GeminiProvider::official_for_test(server.uri())
        .unwrap()
        .generate(
            &request_for_model_with_limits("gemini-2.5-flash", 1, 1),
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
});

shared_runtime_test!(no_or_blank_usable_candidate_text_is_rejected, {
    let cases = [
        json!({}),
        json!({"candidates": []}),
        json!({
            "candidates": [{
                "content": {"parts": [
                    {"text": "  \n  "},
                    {
                        "thought": true,
                        "text": r#"{"what_happened":"Hidden thought.","why_it_matters":"It is not answer text.","caveat":null}"#
                    }
                ]},
                "finishReason": "STOP"
            }]
        }),
    ];

    for body in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let failure = GeminiProvider::official_for_test(server.uri())
            .unwrap()
            .generate(
                &request_for_model("gemini-2.5-flash"),
                &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
            )
            .await
            .unwrap_err();
        assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
    }
});

shared_runtime_test!(
    blank_candidates_are_ignored_and_identical_usable_candidates_are_unambiguous,
    {
        let server = MockServer::start().await;
        let text = r#"{"what_happened":"Matching candidates.","why_it_matters":"They are not ambiguous.","caveat":null}"#;
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [
                    {"content": {"parts": [{"text": "  "}]}, "finishReason": "STOP"},
                    {"content": {"parts": [{"text": text}]}, "finishReason": "STOP"},
                    {"content": {"parts": [{"text": text}]}, "finishReason": "STOP"}
                ]
            })))
            .mount(&server)
            .await;

        let response = GeminiProvider::official_for_test(server.uri())
            .unwrap()
            .generate(
                &request_for_model("gemini-2.5-flash"),
                &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
            )
            .await
            .unwrap();

        assert_eq!(response.fields.what_happened, "Matching candidates.");
    }
);

shared_runtime_test!(
    malformed_and_oversized_success_bodies_use_the_shared_cap_and_remain_redacted,
    {
        const SENTINEL_MALFORMED: &str = "SENTINEL-GEMINI-MALFORMED-BODY";
        const SENTINEL_OVERSIZED: &str = "SENTINEL-GEMINI-OVERSIZED-BODY";
        let bodies = [
            SENTINEL_MALFORMED.to_owned(),
            format!("{SENTINEL_OVERSIZED}{}", "x".repeat(256 * 1024 + 1)),
        ];

        for body in bodies {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
                .respond_with(ResponseTemplate::new(200).set_body_string(body))
                .mount(&server)
                .await;

            let failure = GeminiProvider::official_for_test(server.uri())
                .unwrap()
                .generate(
                    &request_for_model("gemini-2.5-flash"),
                    &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
                )
                .await
                .unwrap_err();
            assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
            let rendered = format!("{failure:?} {failure}");
            assert!(!rendered.contains(SENTINEL_MALFORMED));
            assert!(!rendered.contains(SENTINEL_OVERSIZED));
        }
    }
);

shared_runtime_test!(
    permanent_4xx_is_not_retried_and_exhausted_5xx_is_redacted,
    {
        for (status, expected_kind, expected_attempts) in [
            (400, ProviderFailureKind::ProviderRejected, 1),
            (503, ProviderFailureKind::ProviderUnavailable, 2),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
                .respond_with(
                    ResponseTemplate::new(status)
                        .insert_header("Retry-After", "0")
                        .set_body_string("SENTINEL-EXHAUSTED-GEMINI-BODY"),
                )
                .mount(&server)
                .await;

            let failure = GeminiProvider::official_for_test(server.uri())
                .unwrap()
                .generate(
                    &request_for_model("gemini-2.5-flash"),
                    &ResolvedCredential::new(SENTINEL_SECRET.to_owned()),
                )
                .await
                .unwrap_err();
            assert_eq!(failure.kind(), expected_kind);
            assert_eq!(
                server.received_requests().await.unwrap().len(),
                expected_attempts
            );
            assert!(!format!("{failure:?} {failure}").contains("SENTINEL"));
        }
    }
);

fn assert_sentinel_only_in_x_goog_api_key(request: &Request) {
    assert!(!request.url.as_str().contains(SENTINEL_SECRET));
    assert!(!String::from_utf8_lossy(&request.body).contains(SENTINEL_SECRET));
    assert!(request.headers.get("authorization").is_none());
    for (name, value) in &request.headers {
        if name.as_str() != "x-goog-api-key" {
            assert!(!value.to_str().unwrap_or_default().contains(SENTINEL_SECRET));
        }
    }
}
