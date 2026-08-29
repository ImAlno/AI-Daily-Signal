use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use chrono::{TimeZone, Utc};
use flate2::Compression;
use flate2::write::GzEncoder;
use reqwest::StatusCode;
use serde_json::json;
use signal_core::test_support::{provider_http_client, read_provider_json, story_fixture};
use signal_core::{
    AI_SUMMARY_PROMPT_VERSION, AiSummaryPrompt, GenerationFailureKind, ProviderFailure,
    ProviderFailureKind, ProviderRequest, RequestChargeStatus, RetryAttemptFailure, RetryPolicy,
    RetrySleeper, SignalError, SummarySettings, build_ai_summary_prompt, parse_ai_summary,
    retry_provider_operation, summary_cache_key,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn prompt_contains_only_canonical_approved_story_fields() {
    let mut story = story_fixture("SENTINEL-STORY-ID");
    story.title = "  An AI—Signal!  ".to_owned();
    story.excerpt = " <p>A &amp; B</p>\n  shipped. ".to_owned();
    story.canonical_url = "https://EXAMPLE.com:443/item?utm_source=x&b=2&a=1#part".to_owned();
    story.published_at = Some(Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap());
    story.category = "  model   releases ".to_owned();
    story.source_ids = vec![" z-source ".to_owned(), "a-source".to_owned()];
    story.smart_summary = "SENTINEL-LOCAL-HISTORY".to_owned();
    story.is_read = true;
    story.is_saved = true;

    let prompt = build_ai_summary_prompt(&story, &SummarySettings::default()).unwrap();
    let story_json: serde_json::Value = serde_json::from_str(&prompt.user_text).unwrap();

    assert_eq!(AI_SUMMARY_PROMPT_VERSION, "ai-summary-v1");
    assert_eq!(
        story_json,
        json!({
            "normalized_title": "an ai signal",
            "excerpt": "A & B shipped.",
            "canonical_url": "https://example.com/item?a=1&b=2",
            "published_at": "2026-08-29T08:00:00+00:00",
            "category": "model releases",
            "source_ids": ["a-source", "z-source"]
        })
    );
    let rendered = format!("{} {}", prompt.system_text, prompt.user_text);
    assert!(!rendered.contains("SENTINEL"));
    assert!(!rendered.contains("is_saved"));
    assert!(!rendered.contains("is_read"));
}

#[test]
fn prompt_is_independent_of_cache_and_local_story_metadata() {
    let original = story_fixture("story-one");
    let mut changed = original.clone();
    changed.id = "story-two".to_owned();
    changed.is_read = true;
    changed.is_saved = true;
    changed.smart_summary = "Different local summary".to_owned();
    changed.score.total = 999.0;

    assert_eq!(
        build_ai_summary_prompt(&original, &SummarySettings::default()).unwrap(),
        build_ai_summary_prompt(&changed, &SummarySettings::default()).unwrap()
    );
}

#[test]
fn prompt_canonical_url_never_contains_url_user_info() {
    let mut story = story_fixture("story-one");
    story.canonical_url = "https://SENTINEL-USER:SENTINEL-PASSWORD@example.com/article".to_owned();

    let prompt = build_ai_summary_prompt(&story, &SummarySettings::default()).unwrap();

    assert!(!prompt.user_text.contains("SENTINEL"));
    assert!(prompt.user_text.contains("https://example.com/article"));
}

#[test]
fn excerpt_canonicalization_is_identical_for_prompt_and_cache() {
    let encoded = signal_core::test_support::cache_identity_fixture();
    let mut plain = encoded.clone();
    let mut encoded_story = encoded.story.clone();
    encoded_story.excerpt = "<p>A &amp; <strong>B</strong></p>".to_owned();
    plain.story.excerpt = "A & B".to_owned();

    assert_eq!(
        build_ai_summary_prompt(&encoded_story, &encoded.settings)
            .unwrap()
            .user_text,
        build_ai_summary_prompt(&plain.story, &plain.settings)
            .unwrap()
            .user_text
    );
    assert_eq!(
        summary_cache_key(
            &encoded_story,
            &encoded.profile,
            &encoded.prompt_version,
            &encoded.settings,
        )
        .unwrap(),
        summary_cache_key(
            &plain.story,
            &plain.profile,
            &plain.prompt_version,
            &plain.settings,
        )
        .unwrap()
    );
}

#[test]
fn url_canonicalization_is_identical_for_prompt_and_cache() {
    let with_user_info = signal_core::test_support::cache_identity_fixture();
    let mut canonical = with_user_info.clone();
    let mut protected_story = with_user_info.story.clone();
    protected_story.canonical_url =
        "https://SENTINEL-USER:SENTINEL-PASSWORD@EXAMPLE.com:443/item?utm_source=x&b=2&a=1#part"
            .to_owned();
    canonical.story.canonical_url = "https://example.com/item?a=1&b=2".to_owned();

    assert_eq!(
        build_ai_summary_prompt(&protected_story, &with_user_info.settings)
            .unwrap()
            .user_text,
        build_ai_summary_prompt(&canonical.story, &canonical.settings)
            .unwrap()
            .user_text
    );
    assert_eq!(
        summary_cache_key(
            &protected_story,
            &with_user_info.profile,
            &with_user_info.prompt_version,
            &with_user_info.settings,
        )
        .unwrap(),
        summary_cache_key(
            &canonical.story,
            &canonical.profile,
            &canonical.prompt_version,
            &canonical.settings,
        )
        .unwrap()
    );
}

#[test]
fn provider_request_debug_redacts_story_prompt_model_and_endpoint() {
    let mut profile = signal_core::test_support::model_profile(
        "request-debug",
        signal_core::ProviderKind::OpenAiCompatible,
    );
    profile.model = "SENTINEL-MODEL".to_owned();
    profile.endpoint = Some("https://example.com/SENTINEL-ENDPOINT".parse().unwrap());
    profile.dialect = Some(signal_core::ApiDialect::Responses);
    let prompt = AiSummaryPrompt {
        system_text: "SENTINEL-SYSTEM".to_owned(),
        user_text: "SENTINEL-USER-TEXT".to_owned(),
    };

    let request = ProviderRequest::from_profile("SENTINEL-STORY-ID", &profile, prompt).unwrap();
    let rendered = format!("{request:?}");

    assert!(!rendered.contains("SENTINEL"));
    assert!(!rendered.contains("example.com"));
}

#[test]
fn provider_request_rejects_url_user_info_without_echoing_it() {
    let mut profile = signal_core::test_support::model_profile(
        "request-user-info",
        signal_core::ProviderKind::OpenAiCompatible,
    );
    profile.endpoint = Some(
        "https://SENTINEL-USER:SENTINEL-PASSWORD@example.com/v1"
            .parse()
            .unwrap(),
    );
    profile.dialect = Some(signal_core::ApiDialect::Responses);
    let prompt = AiSummaryPrompt {
        system_text: "system".to_owned(),
        user_text: "user".to_owned(),
    };

    let error = ProviderRequest::from_profile("story", &profile, prompt).unwrap_err();
    let rendered = format!("{error:?} {error}");

    assert!(matches!(error, SignalError::InvalidConfiguration(_)));
    assert!(!rendered.contains("SENTINEL"));
    assert!(!rendered.contains("example.com"));
}

#[test]
fn provider_request_revalidates_a_mutated_nonloopback_http_profile() {
    let mut profile = signal_core::test_support::model_profile(
        "unsafe-request-endpoint",
        signal_core::ProviderKind::OpenAiCompatible,
    );
    profile.endpoint = Some("http://SENTINEL-UNSAFE.example/v1".parse().unwrap());
    profile.dialect = Some(signal_core::ApiDialect::Responses);
    let prompt = AiSummaryPrompt {
        system_text: "system".to_owned(),
        user_text: "user".to_owned(),
    };

    let error = ProviderRequest::from_profile("story", &profile, prompt).unwrap_err();
    let rendered = format!("{error:?} {error}");

    assert!(matches!(error, SignalError::InvalidConfiguration(_)));
    assert!(!rendered.contains("SENTINEL"));
    assert!(!rendered.contains("example"));
}

#[test]
fn strict_parser_accepts_only_the_required_json_shape() {
    let settings = SummarySettings::default();
    let parsed = parse_ai_summary(
        r#"{"what_happened":"A happened.","why_it_matters":"B matters.","caveat":null}"#,
        &settings,
    )
    .unwrap();
    assert_eq!(parsed.what_happened, "A happened.");
    assert_eq!(parsed.why_it_matters, "B matters.");
    assert_eq!(parsed.caveat, None);

    for invalid in [
        r#"{"what_happened":"A","why_it_matters":"B","extra":true}"#,
        r#"```json
{"what_happened":"A","why_it_matters":"B"}
```"#,
        r#"Here is the JSON: {"what_happened":"A","why_it_matters":"B"}"#,
        r#"{"what_happened":"A","why_it_matters":"B"} trailing"#,
        r#"{"what_happened":"","why_it_matters":"B"}"#,
        r#"{"what_happened":"A","why_it_matters":" "}"#,
        r#"{"what_happened":"A","why_it_matters":"B","caveat":7}"#,
    ] {
        let failure = parse_ai_summary(invalid, &settings).unwrap_err();
        assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
        assert_eq!(failure.charge_status(), RequestChargeStatus::PossiblySent);
    }
}

#[test]
fn strict_parser_enforces_scalar_limits_and_rejects_markup() {
    let settings = SummarySettings {
        what_happened_max_chars: 3,
        why_it_matters_max_chars: 3,
        caveat_max_chars: 3,
    };

    for invalid in [
        r#"{"what_happened":"four","why_it_matters":"yes"}"#,
        r#"{"what_happened":"yes","why_it_matters":"four"}"#,
        r#"{"what_happened":"yes","why_it_matters":"yes","caveat":"four"}"#,
        r#"{"what_happened":"<b>x</b>","why_it_matters":"yes"}"#,
        r#"{"what_happened":"[x](https://example.com)","why_it_matters":"yes"}"#,
    ] {
        assert!(parse_ai_summary(invalid, &settings).is_err());
    }
}

#[test]
fn every_provider_failure_kind_has_a_persisted_mapping() {
    let cases = [
        (
            ProviderFailureKind::CredentialMissing,
            GenerationFailureKind::CredentialMissing,
        ),
        (
            ProviderFailureKind::Authentication,
            GenerationFailureKind::Authentication,
        ),
        (
            ProviderFailureKind::RateLimited,
            GenerationFailureKind::RateLimited,
        ),
        (ProviderFailureKind::Timeout, GenerationFailureKind::Timeout),
        (
            ProviderFailureKind::Transport,
            GenerationFailureKind::Transport,
        ),
        (
            ProviderFailureKind::ProviderRejected,
            GenerationFailureKind::ProviderRejected,
        ),
        (
            ProviderFailureKind::ProviderUnavailable,
            GenerationFailureKind::ProviderUnavailable,
        ),
        (
            ProviderFailureKind::MalformedOutput,
            GenerationFailureKind::MalformedOutput,
        ),
    ];

    for (provider, persisted) in cases {
        assert_eq!(GenerationFailureKind::from(provider), persisted);
    }
}

#[test]
fn request_charge_status_distinguishes_pre_send_and_post_send_failures() {
    let pre_send =
        ProviderFailure::new(ProviderFailureKind::Transport, RequestChargeStatus::NotSent);
    let post_send = ProviderFailure::new(
        ProviderFailureKind::Timeout,
        RequestChargeStatus::PossiblySent,
    );

    assert_eq!(pre_send.charge_status(), RequestChargeStatus::NotSent);
    assert_eq!(post_send.charge_status(), RequestChargeStatus::PossiblySent);
}

#[test]
fn provider_failure_debug_never_contains_response_or_credential() {
    let failure = ProviderFailure::for_test("SENTINEL-SECRET", "SENTINEL-BODY");
    let rendered = format!("{failure:?} {failure}");
    assert!(!rendered.contains("SENTINEL"));
}

#[test]
fn retry_classification_is_limited_to_timeout_429_and_5xx() {
    let cases = [
        (ProviderFailureKind::Timeout, true),
        (ProviderFailureKind::RateLimited, true),
        (ProviderFailureKind::ProviderUnavailable, true),
        (ProviderFailureKind::Authentication, false),
        (ProviderFailureKind::ProviderRejected, false),
        (ProviderFailureKind::Transport, false),
        (ProviderFailureKind::MalformedOutput, false),
        (ProviderFailureKind::CredentialMissing, false),
    ];
    for (kind, retryable) in cases {
        assert_eq!(kind.is_retryable(), retryable);
    }

    assert_eq!(
        ProviderFailureKind::from_http_status(StatusCode::UNAUTHORIZED),
        ProviderFailureKind::Authentication
    );
    assert_eq!(
        ProviderFailureKind::from_http_status(StatusCode::TOO_MANY_REQUESTS),
        ProviderFailureKind::RateLimited
    );
    assert_eq!(
        ProviderFailureKind::from_http_status(StatusCode::BAD_GATEWAY),
        ProviderFailureKind::ProviderUnavailable
    );
    assert_eq!(
        ProviderFailureKind::from_http_status(StatusCode::BAD_REQUEST),
        ProviderFailureKind::ProviderRejected
    );
}

#[derive(Default)]
struct RecordingSleeper {
    delays: Mutex<Vec<Duration>>,
}

#[async_trait::async_trait]
impl RetrySleeper for RecordingSleeper {
    async fn sleep(&self, duration: Duration) {
        self.delays.lock().unwrap().push(duration);
    }
}

#[tokio::test]
async fn retry_after_and_total_sleep_are_clamped_to_the_profile_horizon() {
    let sleeper = RecordingSleeper::default();
    let policy = RetryPolicy::new(Duration::from_secs(2), 2);
    let mut attempts = 0;

    let result = retry_provider_operation(&policy, &sleeper, || {
        attempts += 1;
        async move {
            if attempts < 3 {
                Err(RetryAttemptFailure::new(
                    ProviderFailure::new(
                        ProviderFailureKind::RateLimited,
                        RequestChargeStatus::PossiblySent,
                    ),
                    Some(Duration::from_secs(60)),
                ))
            } else {
                Ok("done")
            }
        }
    })
    .await;

    assert_eq!(result.unwrap(), "done");
    assert_eq!(attempts, 3);
    let delays = sleeper.delays.lock().unwrap();
    assert_eq!(delays.iter().sum::<Duration>(), policy.full_horizon());
    assert!(delays.iter().all(|delay| *delay <= policy.full_horizon()));
}

#[tokio::test]
async fn permanent_failures_are_not_retried() {
    let sleeper = RecordingSleeper::default();
    let policy = RetryPolicy::new(Duration::from_secs(2), 3);
    let mut attempts = 0;

    let failure = retry_provider_operation(&policy, &sleeper, || {
        attempts += 1;
        async {
            Err::<(), _>(RetryAttemptFailure::new(
                ProviderFailure::new(
                    ProviderFailureKind::Authentication,
                    RequestChargeStatus::PossiblySent,
                ),
                None,
            ))
        }
    })
    .await
    .unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::Authentication);
    assert_eq!(attempts, 1);
    assert!(sleeper.delays.lock().unwrap().is_empty());
}

#[tokio::test]
async fn possibly_sent_survives_a_later_not_sent_retry_failure() {
    let sleeper = RecordingSleeper::default();
    let policy = RetryPolicy::new(Duration::from_secs(2), 1);
    let mut attempts = 0;

    let failure = retry_provider_operation(&policy, &sleeper, || {
        attempts += 1;
        async move {
            let charge_status = if attempts == 1 {
                RequestChargeStatus::PossiblySent
            } else {
                RequestChargeStatus::NotSent
            };
            Err::<(), _>(RetryAttemptFailure::new(
                ProviderFailure::new(ProviderFailureKind::Timeout, charge_status),
                Some(Duration::ZERO),
            ))
        }
    })
    .await
    .unwrap_err();

    assert_eq!(attempts, 2);
    assert_eq!(failure.charge_status(), RequestChargeStatus::PossiblySent);
}

#[test]
fn retry_after_parsing_accepts_delta_or_http_date_without_echoing_input() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    assert_eq!(
        RetryPolicy::parse_retry_after("12", now),
        Some(Duration::from_secs(12))
    );
    assert_eq!(
        RetryPolicy::parse_retry_after("Mon, 12 Jan 1970 13:46:50 GMT", now),
        Some(Duration::from_secs(10))
    );
    assert_eq!(RetryPolicy::parse_retry_after("SENTINEL-SECRET", now), None);
}

#[test]
fn past_valid_retry_after_http_date_requests_immediate_retry() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);

    assert_eq!(
        RetryPolicy::parse_retry_after("Mon, 12 Jan 1970 13:46:39 GMT", now),
        Some(Duration::ZERO)
    );
}

#[tokio::test]
async fn shared_http_client_refuses_redirects_and_has_no_default_auth() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/start"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/target"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/target"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let response = provider_http_client()
        .unwrap()
        .get(format!("{}/start", server.uri()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .headers
            .get("user-agent")
            .unwrap()
            .to_str()
            .unwrap(),
        "ai-daily-signal/0.1.0"
    );
    assert!(!requests[0].headers.contains_key("authorization"));
}

#[tokio::test]
async fn response_body_is_rejected_above_256_kib_before_json_parsing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/large"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 256 * 1024 + 1]))
        .mount(&server)
        .await;

    let response = provider_http_client()
        .unwrap()
        .get(format!("{}/large", server.uri()))
        .send()
        .await
        .unwrap();
    let failure = read_provider_json(response).await.unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
    assert_eq!(failure.charge_status(), RequestChargeStatus::PossiblySent);
    assert!(!format!("{failure:?} {failure}").contains('x'));
}

#[tokio::test]
async fn decoded_gzip_response_body_is_rejected_above_256_kib() {
    let server = MockServer::start().await;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, &vec![b'x'; 256 * 1024 + 1]).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() < 256 * 1024);

    Mock::given(method("GET"))
        .and(path("/compressed-large"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Encoding", "gzip")
                .set_body_bytes(compressed),
        )
        .mount(&server)
        .await;

    let response = provider_http_client()
        .unwrap()
        .get(format!("{}/compressed-large", server.uri()))
        .send()
        .await
        .unwrap();
    let failure = read_provider_json(response).await.unwrap_err();

    assert_eq!(failure.kind(), ProviderFailureKind::MalformedOutput);
    assert_eq!(failure.charge_status(), RequestChargeStatus::PossiblySent);
}
