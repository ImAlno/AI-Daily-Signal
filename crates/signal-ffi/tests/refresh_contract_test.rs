#![cfg(feature = "test-support")]

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use secrecy::SecretString;
use signal_core::{
    ApiDialect, AppPaths, ConfigRepository, CredentialStore, FeedCollector, GenerationOutcomeKind,
    OpenAiProvider, Pipeline, ProviderKind, ProviderRegistry, SignalApp, SourceKind, Store,
    test_support::{MemoryCredentialStore, MemoryEnvironmentReader},
};
use signal_ffi::{CompanionClient, CompanionError};

const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>FFI refresh fixture</title>
  <item><guid>ffi-refresh-story</guid><title>Deterministic FFI refresh story</title>
    <link>https://example.com/ffi-refresh-story</link>
    <description>A complete deterministic fixture sentence.</description></item>
</channel></rss>"#;

#[derive(Default)]
struct RequestGateState {
    entered: bool,
    released: bool,
    closed: bool,
}

#[derive(Clone)]
struct RequestGate {
    state: Arc<(Mutex<RequestGateState>, Condvar)>,
}

impl RequestGate {
    fn new() -> Self {
        Self {
            state: Arc::new((Mutex::new(RequestGateState::default()), Condvar::new())),
        }
    }

    async fn wait_until_entered(&self) {
        let gate = self.clone();
        let entered = tokio::task::spawn_blocking(move || gate.wait_until_entered_blocking())
            .await
            .expect("request-entry gate task");
        assert!(entered, "gated server closed before a request entered");
    }

    fn wait_until_entered_blocking(&self) -> bool {
        let (state, changed) = &*self.state;
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !state.entered && !state.closed {
            state = changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state.entered
    }

    fn enter_and_wait_for_release(&self, shutdown: &AtomicBool) -> bool {
        let (state, changed) = &*self.state;
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.entered = true;
        changed.notify_all();
        while !state.released && !state.closed {
            state = changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        !state.closed && !shutdown.load(Ordering::SeqCst)
    }

    fn release(&self) {
        let (state, changed) = &*self.state;
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.released = true;
        changed.notify_all();
    }

    fn close(&self) {
        let (state, changed) = &*self.state;
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.closed = true;
        state.released = true;
        changed.notify_all();
    }
}

struct LoopbackServer {
    address: std::net::SocketAddr,
    shutdown: Arc<AtomicBool>,
    requests: Arc<AtomicUsize>,
    gate: Option<RequestGate>,
    thread: Option<JoinHandle<()>>,
}

impl LoopbackServer {
    fn start(status: u16, content_type: &'static str, body: &'static str) -> Self {
        Self::start_with_gate(status, content_type, body, None)
    }

    fn gated(
        status: u16,
        content_type: &'static str,
        body: &'static str,
        gate: &RequestGate,
    ) -> Self {
        Self::start_with_gate(status, content_type, body, Some(gate.clone()))
    }

    fn start_with_gate(
        status: u16,
        content_type: &'static str,
        body: &'static str,
        gate: Option<RequestGate>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback server bind");
        let address = listener.local_addr().expect("loopback server address");
        let shutdown = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let worker_shutdown = shutdown.clone();
        let worker_requests = requests.clone();
        let worker_gate = gate.clone();
        let thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                if worker_shutdown.load(Ordering::SeqCst) {
                    return;
                }
                let request_number = worker_requests.fetch_add(1, Ordering::SeqCst) + 1;
                read_request(&mut stream);
                if request_number == 1
                    && let Some(gate) = &worker_gate
                    && !gate.enter_and_wait_for_release(&worker_shutdown)
                {
                    return;
                }
                let reason = if status == 200 { "OK" } else { "Server Error" };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        Self {
            address,
            shutdown,
            requests,
            gate,
            thread: Some(thread),
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Drop for LoopbackServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.close();
        }
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read_request(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("request read timeout");
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut expected_length = None;
    loop {
        let Ok(count) = stream.read(&mut buffer) else {
            return;
        };
        if count == 0 {
            return;
        }
        request.extend_from_slice(&buffer[..count]);
        if expected_length.is_none()
            && let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
        {
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let body_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            expected_length = Some(header_end + 4 + body_length);
        }
        if expected_length.is_some_and(|length| request.len() >= length) {
            return;
        }
    }
}

struct RefreshFixture {
    _root: tempfile::TempDir,
    store: Store,
    client: Arc<CompanionClient>,
}

impl RefreshFixture {
    fn with_provider(feed_url: String, provider_url: String) -> Self {
        let root = tempfile::tempdir().expect("temporary refresh root");
        let paths = AppPaths::for_root(root.path());
        let mut config = signal_core::test_support::config_fixture();
        config.sources.truncate(1);
        config.briefing.max_items = 1;
        match &mut config.sources[0].kind {
            SourceKind::Feed { url } => *url = feed_url,
        }
        ConfigRepository::new(paths.clone())
            .save(&config)
            .expect("refresh fixture config");

        let store = Store::open(paths.data_dir.join("signal.sqlite3")).expect("refresh store");
        let prior = signal_core::test_support::briefing_fixture();
        store
            .commit_refresh(
                &prior
                    .items
                    .iter()
                    .map(|item| item.story.clone())
                    .collect::<Vec<_>>(),
                &prior,
            )
            .expect("prior refresh fixture");

        let mut profile =
            signal_core::test_support::model_profile("ffi-refresh", ProviderKind::OpenAiCompatible);
        profile.endpoint = Some(
            format!("{provider_url}/v1")
                .parse()
                .expect("provider endpoint"),
        );
        profile.dialect = Some(ApiDialect::Responses);
        profile.limits.input_cost_microusd_per_million = Some(1_000_000);
        profile.limits.output_cost_microusd_per_million = Some(1_000_000);
        store
            .create_model_profile(&profile)
            .expect("refresh model profile");
        store
            .set_default_model_profile(Some(profile.id))
            .expect("default refresh profile");

        let credentials = Arc::new(MemoryCredentialStore::default());
        credentials
            .set(
                &profile.credential,
                SecretString::from("ffi-refresh-fixture-secret".to_owned()),
            )
            .expect("refresh fixture credential");
        let mut providers = ProviderRegistry::new();
        providers.register(
            ProviderKind::OpenAiCompatible,
            Arc::new(OpenAiProvider::compatible().expect("loopback-compatible provider")),
        );
        let app = SignalApp::open_with_services(
            paths,
            credentials,
            Arc::new(MemoryEnvironmentReader::default()),
            Arc::new(providers),
        )
        .expect("refresh fixture app");
        let client = CompanionClient::for_test(app);

        Self {
            _root: root,
            store,
            client,
        }
    }
}

#[test]
fn feed_fixture_remains_briefing_eligible_at_a_far_future_collection_time() {
    // Break caught: a fixed publication date silently expiring after the seven-day window.
    use chrono::{TimeZone, Utc};

    let config = signal_core::test_support::config_fixture();
    let far_future = Utc.with_ymd_and_hms(2036, 8, 30, 12, 0, 0).unwrap();
    let candidates = FeedCollector::parse(&config.sources[0], FEED.as_bytes(), far_future)
        .expect("far-future fixture feed parse");
    let output = Pipeline::build(candidates, &config, far_future);

    assert_eq!(output.briefing.items.len(), 1);
}

#[test]
fn dropping_a_gated_server_before_release_does_not_block_cleanup() {
    // Break caught: assertion unwind hanging forever in LoopbackServer::drop on the release gate.
    let gate = RequestGate::new();
    let server = LoopbackServer::gated(200, "application/rss+xml", FEED, &gate);
    let address = server.address;
    let request = thread::spawn(move || {
        let mut stream = TcpStream::connect(address).expect("gated cleanup request connect");
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .expect("gated cleanup request write");
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
    });
    assert!(gate.wait_until_entered_blocking());

    drop(server);
    request.join().expect("gated cleanup request thread");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn active_refresh_has_exact_single_flight_ownership_and_cancellation_preserves_today() {
    // Break caught: overlapping IDs or a non-owner can cancel, cancellation blocks behind the
    // app mutex, the prior briefing is replaced, or a possibly-sent request loses its charge.
    let feed = LoopbackServer::start(200, "application/rss+xml", FEED);
    let provider_gate = RequestGate::new();
    let provider = LoopbackServer::gated(
        400,
        "application/json",
        "ffi-provider-private-body-SENTINEL",
        &provider_gate,
    );
    let fixture = RefreshFixture::with_provider(feed.url(), provider.url());
    let before = fixture
        .client
        .snapshot()
        .await
        .expect("snapshot before refresh");

    fixture
        .client
        .reserve_refresh("refresh-a".into())
        .expect("reserve first refresh");
    let client = fixture.client.clone();
    let mut first = tokio::spawn(async move { client.refresh("refresh-a".into(), true).await });
    tokio::select! {
        () = provider_gate.wait_until_entered() => {}
        result = &mut first => panic!("refresh completed before provider request: {result:?}"),
    }

    assert_eq!(
        fixture
            .client
            .refresh("refresh-b".into(), true)
            .await
            .unwrap_err(),
        CompanionError::RefreshAlreadyRunning,
    );
    assert_eq!(
        fixture
            .client
            .refresh("refresh-a".into(), true)
            .await
            .unwrap_err(),
        CompanionError::RefreshAlreadyRunning,
    );
    assert!(!fixture.client.cancel_operation("refresh-b".into()));
    assert!(fixture.client.cancel_operation("refresh-a".into()));
    assert!(fixture.client.cancel_operation("refresh-a".into()));
    provider_gate.release();

    assert_eq!(
        first.await.expect("first refresh task").unwrap_err(),
        CompanionError::Cancelled
    );
    assert_eq!(provider.request_count(), 1);
    let attempt = fixture
        .store
        .list_generation_attempts()
        .expect("generation attempts")
        .into_iter()
        .next()
        .expect("possibly-sent generation attempt");
    assert_eq!(
        attempt.final_outcome,
        Some(GenerationOutcomeKind::FailedCharged)
    );
    assert_eq!(
        attempt.actual_cost_microusd,
        Some(attempt.estimated_cost_microusd)
    );
    let after = fixture
        .client
        .snapshot()
        .await
        .expect("snapshot after refresh");
    assert_eq!(after.today, before.today);
    assert!(!fixture.client.cancel_operation("refresh-a".into()));
}

#[tokio::test]
async fn reserved_refresh_cancels_before_async_work_and_releases_exact_ownership() {
    // Break caught: cancellation arriving before the exported async refresh body registers its
    // operation, allowing HTTP and a refresh commit to proceed without a cancellable owner.
    let feed = LoopbackServer::start(200, "application/rss+xml", FEED);
    let unused_provider = LoopbackServer::start(200, "application/json", "unused");
    let fixture = RefreshFixture::with_provider(feed.url(), unused_provider.url());
    let before = fixture
        .client
        .state_revision()
        .await
        .expect("initial revision");

    fixture
        .client
        .reserve_refresh("reserved-refresh".into())
        .expect("reserve synchronously before async hand-off");
    assert_eq!(
        fixture
            .client
            .reserve_refresh("overlap".into())
            .unwrap_err(),
        CompanionError::RefreshAlreadyRunning
    );
    assert!(!fixture.client.cancel_operation("wrong-refresh".into()));
    assert!(fixture.client.cancel_operation("reserved-refresh".into()));
    assert!(fixture.client.cancel_operation("reserved-refresh".into()));
    assert_eq!(
        fixture
            .client
            .refresh("reserved-refresh".into(), false)
            .await
            .unwrap_err(),
        CompanionError::Cancelled
    );
    assert_eq!(feed.request_count(), 0);
    assert_eq!(fixture.client.state_revision().await.unwrap(), before);
    assert!(!fixture.client.cancel_operation("reserved-refresh".into()));

    fixture
        .client
        .reserve_refresh("released-refresh".into())
        .expect("later reservation after cancellation");
    assert!(
        !fixture
            .client
            .release_refresh_reservation("wrong-refresh".into())
    );
    assert!(
        fixture
            .client
            .release_refresh_reservation("released-refresh".into())
    );
    assert!(!fixture.client.cancel_operation("released-refresh".into()));
    assert_eq!(
        fixture
            .client
            .refresh("released-refresh".into(), false)
            .await
            .unwrap_err(),
        CompanionError::InvalidInput
    );
    assert_eq!(feed.request_count(), 0);

    fixture
        .client
        .reserve_refresh("cancelled-before-handoff".into())
        .expect("reserve a refresh that will be abandoned before hand-off");
    assert!(
        fixture
            .client
            .cancel_operation("cancelled-before-handoff".into())
    );
    assert!(
        fixture
            .client
            .release_refresh_reservation("cancelled-before-handoff".into())
    );
    assert!(
        !fixture
            .client
            .cancel_operation("cancelled-before-handoff".into())
    );

    fixture
        .client
        .reserve_refresh("completed-refresh".into())
        .expect("reserve the later refresh");
    fixture
        .client
        .refresh("completed-refresh".into(), false)
        .await
        .expect("no leaked reservation blocks a later refresh");
    assert_eq!(feed.request_count(), 1);
    assert!(!fixture.client.cancel_operation("completed-refresh".into()));
}

#[tokio::test]
async fn provider_failure_returns_a_successful_smart_fallback_report() {
    // Break caught: a paid-provider failure escaping as an FFI error or losing fallback counts.
    let feed = LoopbackServer::start(200, "application/rss+xml", FEED);
    let provider = LoopbackServer::start(
        400,
        "application/json",
        "ffi-provider-fallback-private-body-SENTINEL",
    );
    let fixture = RefreshFixture::with_provider(feed.url(), provider.url());

    fixture
        .client
        .reserve_refresh("provider-fallback".into())
        .expect("reserve provider-fallback refresh");
    let result = fixture
        .client
        .refresh("provider-fallback".into(), true)
        .await
        .expect("provider failure remains a successful feed refresh");

    assert_eq!(result.successful_sources, 1);
    assert_eq!(result.failed_sources, 0);
    assert_eq!(result.generation.eligible, 1);
    assert_eq!(result.generation.generated, 0);
    assert_eq!(result.generation.provider_failures, 1);
    assert_eq!(result.generation.smart_fallbacks, 1);
    assert_eq!(result.briefing.items.len(), 1);
    assert!(result.briefing.items[0].selected_summary.is_none());
    assert_eq!(
        result.revision,
        fixture.client.state_revision().await.unwrap()
    );
}

#[tokio::test]
async fn total_source_failure_is_offline_keeps_cached_today_and_releases_ownership() {
    // Break caught: total collection failure clearing Today, leaking diagnostics, or leaving busy.
    let failed_feed = LoopbackServer::start(200, "application/rss+xml", "not a feed");
    let unused_provider = LoopbackServer::start(200, "application/json", "unused");
    let fixture = RefreshFixture::with_provider(failed_feed.url(), unused_provider.url());
    let before = fixture.client.snapshot().await.expect("cached snapshot");

    fixture
        .client
        .reserve_refresh("source-failure-a".into())
        .expect("reserve first failing refresh");
    assert_eq!(
        fixture
            .client
            .refresh("source-failure-a".into(), false)
            .await
            .unwrap_err(),
        CompanionError::Offline
    );
    fixture
        .client
        .reserve_refresh("source-failure-b".into())
        .expect("reserve second failing refresh");
    assert_eq!(
        fixture
            .client
            .refresh("source-failure-b".into(), false)
            .await
            .unwrap_err(),
        CompanionError::Offline
    );
    let after = fixture.client.snapshot().await.expect("preserved snapshot");
    assert_eq!(after.today, before.today);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aborting_the_refresh_future_releases_single_flight_ownership() {
    // Break caught: dropping an in-flight UniFFI future leaving the client permanently busy.
    let feed_gate = RequestGate::new();
    let feed = LoopbackServer::gated(200, "application/rss+xml", FEED, &feed_gate);
    let unused_provider = LoopbackServer::start(200, "application/json", "unused");
    let fixture = RefreshFixture::with_provider(feed.url(), unused_provider.url());

    fixture
        .client
        .reserve_refresh("aborted-refresh".into())
        .expect("reserve refresh before its future is spawned");
    let client = fixture.client.clone();
    let mut first =
        tokio::spawn(async move { client.refresh("aborted-refresh".into(), false).await });
    tokio::select! {
        () = feed_gate.wait_until_entered() => {}
        result = &mut first => panic!("refresh completed before feed request: {result:?}"),
    }
    first.abort();
    assert!(
        first
            .await
            .expect_err("aborted refresh task")
            .is_cancelled()
    );
    assert!(!fixture.client.cancel_operation("aborted-refresh".into()));
    feed_gate.release();

    fixture
        .client
        .reserve_refresh("replacement-refresh".into())
        .expect("reserve replacement after abort cleanup");
    let result = fixture
        .client
        .refresh("replacement-refresh".into(), false)
        .await
        .expect("replacement refresh after abort");
    assert_eq!(result.successful_sources, 1);
}
