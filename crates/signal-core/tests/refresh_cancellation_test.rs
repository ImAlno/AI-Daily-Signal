use std::sync::{
    Arc, Mutex, MutexGuard, OnceLock,
    atomic::{AtomicUsize, Ordering},
};

use chrono::{Duration, TimeZone, Utc};
use serde_json::json;
use signal_core::{
    AddModelCredential, AddModelInput, AppConfig, AppPaths, Briefing, BriefingConfig, BriefingItem,
    CancellationToken, ConfigRepository, GenerationOutcomeKind, ProfileLimits, ProviderFailure,
    ProviderFailureKind, ProviderKind, RefreshOptions, RequestChargeStatus, RetryAttemptFailure,
    RetryPolicy, ScoreBreakdown, SignalApp, SignalError, Source, SourceKind, Store, Story,
    TokioRetrySleeper, retry_provider_operation_with_cancel,
};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

static SIGNAL_HOME_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>Cancellation fixture</title>
  <item><guid>fixture-story</guid><title>Deterministic cancellation story</title>
    <link>https://example.com/cancellation-story</link>
    <description>A complete deterministic fixture sentence.</description>
    <pubDate>Sat, 29 Aug 2026 11:30:00 +0000</pubDate></item>
</channel></rss>"#;

const MULTI_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>Cancellation fixture</title>
  <item><guid>fixture-first</guid><title>First deterministic cancellation story</title>
    <link>https://example.com/cancellation-first</link>
    <description>A complete first deterministic fixture sentence.</description>
    <pubDate>Sat, 29 Aug 2026 11:30:00 +0000</pubDate></item>
  <item><guid>fixture-second</guid><title>Second deterministic cancellation story</title>
    <link>https://example.com/cancellation-second</link>
    <description>A complete second deterministic fixture sentence.</description>
    <pubDate>Sat, 29 Aug 2026 10:30:00 +0000</pubDate></item>
</channel></rss>"#;

struct CancelOnRequest {
    token: CancellationToken,
    body: String,
    content_type: &'static str,
}

impl Respond for CancelOnRequest {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        self.token.cancel();
        ResponseTemplate::new(200)
            .insert_header("content-type", self.content_type)
            .set_body_string(&self.body)
    }
}

struct CancelOnNthRequest {
    token: CancellationToken,
    cancel_on: usize,
    requests: AtomicUsize,
    status: u16,
    body: String,
    content_type: &'static str,
}

impl Respond for CancelOnNthRequest {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let request = self.requests.fetch_add(1, Ordering::SeqCst) + 1;
        if request == self.cancel_on {
            self.token.cancel();
        }
        ResponseTemplate::new(self.status)
            .insert_header("content-type", self.content_type)
            .set_body_string(&self.body)
    }
}

struct AppFixture {
    _signal_home_lock: MutexGuard<'static, ()>,
    _root: tempfile::TempDir,
    now: chrono::DateTime<Utc>,
    app: SignalApp,
    store: Store,
}

impl AppFixture {
    fn new(sources: Vec<Source>) -> Self {
        Self::new_with_max_items(sources, 1)
    }

    fn new_with_max_items(sources: Vec<Source>, max_items: usize) -> Self {
        let signal_home_lock = SIGNAL_HOME_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = tempfile::tempdir().expect("temporary app root");
        let paths = AppPaths::for_root(root.path());
        // SAFETY: this fixture holds the process-wide test lock until it is dropped, and this
        // integration-test process does not spawn threads that read this application directory.
        unsafe { std::env::set_var("SIGNAL_HOME", root.path()) };
        let config = AppConfig {
            briefing: BriefingConfig {
                max_items,
                stale_after_minutes: 60,
            },
            sources,
        };
        ConfigRepository::new(paths.clone())
            .save(&config)
            .expect("fixture config");
        let app = SignalApp::open().expect("fixture app");
        let store = Store::open(paths.data_dir.join("signal.sqlite3")).expect("fixture store");

        Self {
            _signal_home_lock: signal_home_lock,
            _root: root,
            now: Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap(),
            app,
            store,
        }
    }

    fn configure_loopback_model(&self, endpoint: &str) {
        // SAFETY: the fixture's process-wide lock also protects this test-only credential.
        unsafe { std::env::set_var("SIGNAL_CANCELLATION_TEST_KEY", "fixture-secret") };
        let profile = self
            .app
            .add_model(
                AddModelInput {
                    name: "cancellation-model".to_owned(),
                    provider: ProviderKind::OpenAiCompatible,
                    model: "fixture-model".to_owned(),
                    endpoint: Some(endpoint.parse().expect("loopback provider endpoint")),
                    dialect: Some(signal_core::ApiDialect::Responses),
                    credential: AddModelCredential::Environment {
                        variable: "SIGNAL_CANCELLATION_TEST_KEY".to_owned(),
                    },
                    consented_at: Some(self.now),
                    enabled: true,
                    limits: ProfileLimits::default(),
                },
                self.now,
            )
            .expect("fixture model")
            .profile;
        self.app
            .use_model(&profile.name)
            .expect("default fixture model");
    }

    fn seed_previous_briefing(&self) -> Briefing {
        let story = Story {
            id: "previous-story".to_owned(),
            title: "Previous successful briefing".to_owned(),
            canonical_url: "https://example.com/previous".to_owned(),
            excerpt: "The prior briefing must remain available.".to_owned(),
            category: "research".to_owned(),
            published_at: Some(self.now - Duration::hours(1)),
            source_ids: vec!["previous-source".to_owned()],
            score: ScoreBreakdown {
                recency: 1.0,
                source_weight: 1.0,
                corroboration: 0.0,
                total: 2.0,
            },
            smart_summary: "Prior summary.".to_owned(),
            is_read: false,
            is_saved: false,
        };
        let briefing = Briefing {
            date: self.now.date_naive(),
            generated_at: self.now - Duration::minutes(1),
            items: vec![BriefingItem {
                position: 1,
                section: "top_signals".to_owned(),
                is_stale: false,
                story: story.clone(),
                selected_summary: None,
            }],
        };
        self.store
            .commit_refresh(&[story], &briefing)
            .expect("prior briefing");
        briefing
    }
}

fn source(id: &str, url: String) -> Source {
    Source {
        id: id.to_owned(),
        name: id.to_owned(),
        category: "research".to_owned(),
        enabled: true,
        weight: 1.0,
        kind: SourceKind::Feed { url },
    }
}

#[test]
fn cancellation_token_is_cloneable_and_monotonic() {
    let token = CancellationToken::new();
    let observer = token.clone();

    assert!(!observer.is_cancelled());
    token.cancel();

    assert!(observer.is_cancelled());
    assert!(matches!(observer.check(), Err(SignalError::Cancelled)));
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_before_collection_makes_no_request_and_records_no_refresh_run() {
    // Break caught: a cancelled refresh starting collection or recording a failed run.
    let feed_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FEED))
        .mount(&feed_server)
        .await;
    let fixture = AppFixture::new(vec![source("first", feed_server.uri())]);
    let token = CancellationToken::new();
    token.cancel();

    let result = fixture
        .app
        .refresh_with_control(fixture.now, RefreshOptions { ai: false }, &token)
        .await;

    assert!(matches!(result, Err(SignalError::Cancelled)));
    assert!(feed_server.received_requests().await.unwrap().is_empty());
    assert!(fixture.store.latest_refresh_run().unwrap().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_after_one_source_stops_before_the_next_source_request() {
    // Break caught: collection continuing to a later source after cancellation.
    let token = CancellationToken::new();
    let first_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(CancelOnRequest {
            token: token.clone(),
            body: FEED.to_owned(),
            content_type: "application/rss+xml",
        })
        .mount(&first_server)
        .await;
    let second_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FEED))
        .mount(&second_server)
        .await;
    let collector = signal_core::FeedCollector::new().unwrap();

    let result = collector
        .collect_all_with_cancel(
            &[
                source("first", first_server.uri()),
                source("second", second_server.uri()),
            ],
            Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap(),
            &token,
        )
        .await;

    assert!(matches!(result, Err(SignalError::Cancelled)));
    assert_eq!(first_server.received_requests().await.unwrap().len(), 1);
    assert!(second_server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_before_ai_dispatch_makes_zero_provider_requests() {
    // Break caught: dispatching an AI request after collection has observed cancellation.
    let token = CancellationToken::new();
    let feed_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(CancelOnRequest {
            token: token.clone(),
            body: FEED.to_owned(),
            content_type: "application/rss+xml",
        })
        .mount(&feed_server)
        .await;
    let provider_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "completed",
            "output": [],
        })))
        .mount(&provider_server)
        .await;
    let fixture = AppFixture::new(vec![source("first", feed_server.uri())]);
    fixture.configure_loopback_model(&provider_server.uri());

    let result = fixture
        .app
        .refresh_with_control(fixture.now, RefreshOptions::default(), &token)
        .await;

    assert!(matches!(result, Err(SignalError::Cancelled)));
    assert!(
        provider_server
            .received_requests()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_after_possibly_sent_provider_failure_finalizes_charge_and_preserves_prior_briefing()
 {
    // Break caught: discarding a possibly charged generation or replacing the last briefing.
    let token = CancellationToken::new();
    let feed_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FEED))
        .mount(&feed_server)
        .await;
    let provider_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(CancelOnRequest {
            token: token.clone(),
            body: "not a provider response".to_owned(),
            content_type: "application/json",
        })
        .mount(&provider_server)
        .await;
    let fixture = AppFixture::new(vec![source("first", feed_server.uri())]);
    fixture.configure_loopback_model(&provider_server.uri());
    let prior = fixture.seed_previous_briefing();

    let result = fixture
        .app
        .refresh_with_control(fixture.now, RefreshOptions::default(), &token)
        .await;

    assert!(matches!(result, Err(SignalError::Cancelled)));
    assert_eq!(provider_server.received_requests().await.unwrap().len(), 1);
    let attempts = fixture.store.list_generation_attempts().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        attempts[0].final_outcome,
        Some(GenerationOutcomeKind::FailedCharged)
    );
    assert_eq!(
        attempts[0].actual_cost_microusd,
        Some(attempts[0].estimated_cost_microusd)
    );
    assert_eq!(fixture.store.load_latest_briefing().unwrap(), Some(prior));
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_retry_stops_before_a_second_provider_request_and_keeps_the_charge() {
    // Break caught: retrying an already-cancelled provider request after a possibly-sent failure.
    let token = CancellationToken::new();
    let feed_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FEED))
        .mount(&feed_server)
        .await;
    let provider_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(CancelOnNthRequest {
            token: token.clone(),
            cancel_on: 1,
            requests: AtomicUsize::new(0),
            status: 429,
            body: "retry later".to_owned(),
            content_type: "application/json",
        })
        .mount(&provider_server)
        .await;
    let fixture = AppFixture::new(vec![source("first", feed_server.uri())]);
    fixture.configure_loopback_model(&provider_server.uri());

    let result = fixture
        .app
        .refresh_with_control(fixture.now, RefreshOptions::default(), &token)
        .await;

    assert!(matches!(result, Err(SignalError::Cancelled)));
    assert_eq!(provider_server.received_requests().await.unwrap().len(), 1);
    let attempt = fixture.store.list_generation_attempts().unwrap().remove(0);
    assert_eq!(
        attempt.final_outcome,
        Some(GenerationOutcomeKind::FailedCharged)
    );
    assert_eq!(
        attempt.actual_cost_microusd,
        Some(attempt.estimated_cost_microusd)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn retry_helper_checks_cancellation_before_each_operation() {
    // Break caught: dispatching a retry after its token has been cancelled.
    let token = CancellationToken::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let operation_calls = calls.clone();
    let operation_token = token.clone();
    let policy = RetryPolicy::new(std::time::Duration::from_secs(1), 1);

    let result: std::result::Result<(), ProviderFailure> =
        retry_provider_operation_with_cancel(&policy, &TokioRetrySleeper, &token, move || {
            let calls = operation_calls.clone();
            let token = operation_token.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                token.cancel();
                Err(RetryAttemptFailure::new(
                    ProviderFailure::new(
                        ProviderFailureKind::RateLimited,
                        RequestChargeStatus::PossiblySent,
                    ),
                    Some(std::time::Duration::ZERO),
                ))
            }
        })
        .await;

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let failure = result.expect_err("cancellation should prevent the retry dispatch");
    assert_eq!(failure.charge_status(), RequestChargeStatus::PossiblySent);
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_after_staged_first_variant_preserves_prior_generation_and_all_variants() {
    // Break caught: persisting part of a multi-item refresh before its final commit boundary.
    let token = CancellationToken::new();
    let feed_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(MULTI_FEED))
        .mount(&feed_server)
        .await;
    let provider_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(CancelOnNthRequest {
            token: token.clone(),
            cancel_on: 2,
            requests: AtomicUsize::new(0),
            status: 200,
            body: json!({
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": r#"{"what_happened":"A deterministic event happened.","why_it_matters":"It proves staged variants.","caveat":null}"#
                    }]
                }],
                "usage": {"input_tokens": 11, "output_tokens": 7}
            })
            .to_string(),
            content_type: "application/json",
        })
        .mount(&provider_server)
        .await;
    let fixture = AppFixture::new_with_max_items(vec![source("first", feed_server.uri())], 2);
    fixture.configure_loopback_model(&provider_server.uri());
    let prior = fixture.seed_previous_briefing();
    let prior_generation = fixture.store.status().unwrap().data_generation;

    let result = fixture
        .app
        .refresh_with_control(fixture.now, RefreshOptions::default(), &token)
        .await;

    assert!(matches!(result, Err(SignalError::Cancelled)));
    assert_eq!(provider_server.received_requests().await.unwrap().len(), 2);
    assert_eq!(
        fixture.store.status().unwrap().data_generation,
        prior_generation
    );
    assert_eq!(fixture.store.load_latest_briefing().unwrap(), Some(prior));
    assert_eq!(fixture.store.list_latest().unwrap().len(), 1);
}
