#![cfg(feature = "test-support")]

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use secrecy::SecretString;
use signal_core::{
    ApiDialect, AppPaths, ConfigRepository, CredentialStore, GenerationOutcomeKind, OpenAiProvider,
    ProviderFailureKind, ProviderKind, ProviderRegistry, SignalApp, SourceKind, Store,
    test_support::{MemoryCredentialStore, MemoryEnvironmentReader},
};
use signal_ffi::{CompanionClient, CompanionError};

const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>FFI refresh fixture</title>
  <item><guid>ffi-refresh-story</guid><title>Deterministic FFI refresh story</title>
    <link>https://example.com/ffi-refresh-story</link>
    <description>A complete deterministic fixture sentence.</description>
    <pubDate>Sat, 29 Aug 2026 11:30:00 +0000</pubDate></item>
</channel></rss>"#;

struct RequestGate {
    entered: Arc<Barrier>,
    release: Arc<Barrier>,
}

impl RequestGate {
    fn new() -> Self {
        Self {
            entered: Arc::new(Barrier::new(2)),
            release: Arc::new(Barrier::new(2)),
        }
    }

    async fn wait_until_entered(&self) {
        let entered = self.entered.clone();
        tokio::task::spawn_blocking(move || {
            entered.wait();
        })
        .await
        .expect("request-entry barrier task");
    }

    async fn release(&self) {
        let release = self.release.clone();
        tokio::task::spawn_blocking(move || {
            release.wait();
        })
        .await
        .expect("request-release barrier task");
    }
}

struct LoopbackServer {
    address: std::net::SocketAddr,
    shutdown: Arc<AtomicBool>,
    requests: Arc<AtomicUsize>,
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
        Self::start_with_gate(
            status,
            content_type,
            body,
            Some((gate.entered.clone(), gate.release.clone())),
        )
    }

    fn start_with_gate(
        status: u16,
        content_type: &'static str,
        body: &'static str,
        gate: Option<(Arc<Barrier>, Arc<Barrier>)>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback server bind");
        let address = listener.local_addr().expect("loopback server address");
        let shutdown = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let worker_shutdown = shutdown.clone();
        let worker_requests = requests.clone();
        let thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                if worker_shutdown.load(Ordering::SeqCst) {
                    return;
                }
                let request_number = worker_requests.fetch_add(1, Ordering::SeqCst) + 1;
                read_request(&mut stream);
                if request_number == 1
                    && let Some((entered, release)) = &gate
                {
                    entered.wait();
                    release.wait();
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

    let client = fixture.client.clone();
    let first = tokio::spawn(async move { client.refresh("refresh-a".into(), true).await });
    provider_gate.wait_until_entered().await;

    assert_eq!(
        fixture
            .client
            .refresh("refresh-b".into(), true)
            .await
            .unwrap_err(),
        CompanionError::RefreshAlreadyRunning,
    );
    assert!(!fixture.client.cancel_operation("refresh-b".into()));
    assert!(fixture.client.cancel_operation("refresh-a".into()));
    assert!(fixture.client.cancel_operation("refresh-a".into()));
    provider_gate.release().await;

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
async fn provider_failure_returns_a_successful_smart_fallback_report() {
    // Break caught: a paid-provider failure escaping as an FFI error or losing fallback counts.
    let core_fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_provider_failure(ProviderFailureKind::ProviderUnavailable);
    let client = CompanionClient::for_test(core_fixture.app);

    let result = client
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
    assert_eq!(result.revision, client.state_revision().await.unwrap());
}

#[tokio::test]
async fn total_source_failure_is_offline_keeps_cached_today_and_releases_ownership() {
    // Break caught: total collection failure clearing Today, leaking diagnostics, or leaving busy.
    let failed_feed = LoopbackServer::start(200, "application/rss+xml", "not a feed");
    let unused_provider = LoopbackServer::start(200, "application/json", "unused");
    let fixture = RefreshFixture::with_provider(failed_feed.url(), unused_provider.url());
    let before = fixture.client.snapshot().await.expect("cached snapshot");

    assert_eq!(
        fixture
            .client
            .refresh("source-failure-a".into(), false)
            .await
            .unwrap_err(),
        CompanionError::Offline
    );
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

    let client = fixture.client.clone();
    let first = tokio::spawn(async move { client.refresh("aborted-refresh".into(), false).await });
    feed_gate.wait_until_entered().await;
    first.abort();
    assert!(
        first
            .await
            .expect_err("aborted refresh task")
            .is_cancelled()
    );
    assert!(!fixture.client.cancel_operation("aborted-refresh".into()));
    feed_gate.release().await;

    let result = fixture
        .client
        .refresh("replacement-refresh".into(), false)
        .await
        .expect("replacement refresh after abort");
    assert_eq!(result.successful_sources, 1);
}
