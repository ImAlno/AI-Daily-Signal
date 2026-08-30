#![cfg(feature = "test-support")]

use std::{
    fs::OpenOptions,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
};

use chrono::Utc;
use fs2::FileExt;
use sha2::Digest;
use signal_core::{
    AppPaths, CONFIG_LOCK_FILE_NAME, ConfigRepository, ProviderRegistry, SignalApp, Store,
    test_support::{MemoryCredentialStore, MemoryEnvironmentReader},
};
use signal_ffi::{AddFeedSourceRequest, CompanionClient, FfiSourceOrigin};

const OVERLAP_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>Shared process fixture</title>
  <item><guid>shared-process-story</guid><title>Shared process source signal</title>
    <link>https://example.com/shared-process-story</link>
    <description>A deterministic shared process fixture sentence.</description></item>
</channel></rss>"#;

struct CountingFeedServer {
    address: std::net::SocketAddr,
    shutdown: Arc<AtomicBool>,
    requests: Arc<AtomicUsize>,
    thread: Option<JoinHandle<()>>,
}

impl CountingFeedServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback feed bind");
        let address = listener.local_addr().expect("loopback feed address");
        let shutdown = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let worker_shutdown = Arc::clone(&shutdown);
        let worker_requests = Arc::clone(&requests);
        let thread = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                if worker_shutdown.load(Ordering::SeqCst) {
                    return;
                }
                worker_requests.fetch_add(1, Ordering::SeqCst);
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/rss+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{OVERLAP_FEED}",
                    OVERLAP_FEED.len()
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
        format!("http://{}/feed.xml", self.address)
    }

    fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Drop for CountingFeedServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct SharedProcessFixture {
    fixture_root: tempfile::TempDir,
    application_root: PathBuf,
    paths: AppPaths,
    client: Arc<CompanionClient>,
    cli_binary: PathBuf,
}

impl SharedProcessFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("isolated application root");
        let application_root = root.path().join("application");
        let cli_binary = build_fresh_cli(root.path());
        let paths = AppPaths::for_root(&application_root);
        ConfigRepository::new(paths.clone())
            .load_or_create()
            .expect("source configuration");
        let store = Store::open(paths.data_dir.join("signal.sqlite3")).expect("SQLite store");
        let mut briefing = signal_core::test_support::briefing_fixture();
        briefing.date = Utc::now().date_naive();
        briefing.generated_at = Utc::now();
        let stories = briefing
            .items
            .iter()
            .map(|item| item.story.clone())
            .collect::<Vec<_>>();
        store
            .commit_refresh(&stories, &briefing)
            .expect("seed briefing");
        drop(store);

        let app = SignalApp::open_with_services(
            paths.clone(),
            Arc::new(MemoryCredentialStore::default()),
            Arc::new(MemoryEnvironmentReader::default()),
            Arc::new(ProviderRegistry::new()),
        )
        .expect("bridge application");
        let client = CompanionClient::for_test(app);

        Self {
            fixture_root: root,
            application_root,
            paths,
            client,
            cli_binary,
        }
    }

    fn root(&self) -> &std::path::Path {
        &self.application_root
    }

    fn cli(&self, args: &[&str]) -> std::process::Output {
        let output = Command::new(&self.cli_binary)
            .env("SIGNAL_HOME", self.root())
            .args(args)
            .output()
            .expect("run real signal CLI process");
        assert!(
            output.status.success(),
            "signal {:?} failed: stdout={} stderr={}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn cli_text(&self, args: &[&str]) -> String {
        String::from_utf8(self.cli(args).stdout).expect("UTF-8 CLI output")
    }

    fn spawn_cli(&self, args: &[&str]) -> std::process::Child {
        Command::new(&self.cli_binary)
            .env("SIGNAL_HOME", self.root())
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn real signal CLI process")
    }
}

fn build_fresh_cli(fixture_root: &std::path::Path) -> PathBuf {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .to_path_buf();
    let target = fixture_root.join("cargo-target");
    let build = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .env_remove("CARGO_TARGET_DIR")
        .args([
            "build",
            "--locked",
            "--quiet",
            "-p",
            "signal-cli",
            "--bin",
            "signal",
            "--target-dir",
        ])
        .arg(&target)
        .output()
        .expect("build fixture-owned real CLI");
    assert!(
        build.status.success(),
        "fixture CLI build failed: stdout={} stderr={}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let binary = target.join("debug").join(if cfg!(windows) {
        "signal.exe"
    } else {
        "signal"
    });
    assert!(
        binary.is_file(),
        "fixture-owned CLI build did not produce {}",
        binary.display()
    );
    let version = Command::new(&binary)
        .arg("--version")
        .output()
        .expect("read fixture CLI version");
    assert!(version.status.success(), "fixture CLI --version failed");
    assert_eq!(
        String::from_utf8(version.stdout)
            .expect("UTF-8 CLI version")
            .trim(),
        format!("signal {}", env!("CARGO_PKG_VERSION")),
        "fixture CLI must be bound to this workspace version"
    );
    binary
}

fn data_generation(status: &str) -> u64 {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Data generation: "))
        .expect("CLI status data generation")
        .parse()
        .expect("numeric data generation")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bridge_and_real_cli_share_one_uncorrupted_application_root() {
    // Break caught: app and CLI drifting into separate stores, stale config, or unsafe concurrent
    // writes despite resolving one explicit application root.
    let fixture = SharedProcessFixture::new();
    assert!(
        fixture
            .cli_binary
            .starts_with(fixture.fixture_root.path().join("cargo-target")),
        "the contract must use a fixture-owned, freshly built CLI artifact"
    );
    let initial = fixture
        .client
        .state_revision()
        .await
        .expect("initial revision");

    let saved = fixture
        .client
        .set_story_saved("story-1".to_owned(), true)
        .await
        .expect("bridge saves story");
    let read = fixture
        .client
        .set_story_read("story-1".to_owned(), true)
        .await
        .expect("bridge marks story read");
    assert!(read.revision.data_generation > saved.revision.data_generation);
    let cli_story = fixture.cli_text(&["--json", "show", "story-1"]);
    assert!(cli_story.contains("\"is_read\": true"));
    assert!(cli_story.contains("\"is_saved\": true"));
    assert!(
        fixture
            .cli_text(&["saved"])
            .contains("A deterministic signal")
    );

    fixture.cli(&["save", "story-1", "--remove"]);
    let after_cli_unsave = fixture
        .client
        .snapshot()
        .await
        .expect("bridge reload after CLI save");
    assert!(!after_cli_unsave.latest[0].is_saved);
    assert!(after_cli_unsave.latest[0].is_read);
    assert!(after_cli_unsave.saved.is_empty());

    let good_feed = CountingFeedServer::start();
    let standard_source_id = after_cli_unsave
        .sources
        .iter()
        .find(|source| source.origin == FfiSourceOrigin::Standard)
        .expect("standard source")
        .id
        .clone();
    let config_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(fixture.paths.config_dir.join(CONFIG_LOCK_FILE_NAME))
        .expect("open stable source configuration lock");
    FileExt::lock_exclusive(&config_lock).expect("hold deterministic config overlap barrier");
    let mut cli_source = fixture.spawn_cli(&["sources", "disable", &standard_source_id]);
    let concurrent_client = Arc::clone(&fixture.client);
    let good_feed_url = good_feed.url();
    let bridge_source = tokio::spawn(async move {
        concurrent_client
            .add_feed_source(AddFeedSourceRequest {
                name: "Shared process feed".to_owned(),
                category: "research".to_owned(),
                url: good_feed_url,
                weight: 0.75,
                enabled: true,
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(
        cli_source
            .try_wait()
            .expect("inspect config-blocked CLI")
            .is_none()
            && !bridge_source.is_finished(),
        "real CLI and bridge source writers must both wait behind the held config barrier"
    );
    FileExt::unlock(&config_lock).expect("release deterministic config overlap barrier");
    let source = tokio::time::timeout(std::time::Duration::from_secs(6), bridge_source)
        .await
        .expect("bridge source writer completes after barrier release")
        .expect("bridge source writer task remains healthy")
        .expect("bridge source write after deterministic overlap");
    let cli_source_output = cli_source
        .wait_with_output()
        .expect("wait for overlapped CLI source write");
    assert!(
        cli_source_output.status.success(),
        "overlapped CLI source write failed: stdout={} stderr={}",
        String::from_utf8_lossy(&cli_source_output.stdout),
        String::from_utf8_lossy(&cli_source_output.stderr)
    );
    assert_eq!(source.source.origin, FfiSourceOrigin::Personal);
    let after_cli_source = fixture
        .client
        .snapshot()
        .await
        .expect("bridge reload after overlapping source writes");
    assert!(
        after_cli_source
            .sources
            .iter()
            .any(|value| value.id == standard_source_id && !value.enabled)
    );
    assert!(
        after_cli_source.sources.iter().any(|value| {
            value.id == source.source.id
                && value.enabled
                && value.origin == FfiSourceOrigin::Personal
        }),
        "the independently added personal source must survive the CLI whole-file mutation"
    );
    assert_ne!(
        after_cli_source.revision.source_config_revision,
        initial.source_config_revision
    );
    assert!(
        fixture
            .cli_text(&["sources", "list"])
            .contains("Shared process feed")
    );

    let stale_feed = CountingFeedServer::start();
    let stale_source = fixture
        .client
        .add_feed_source(AddFeedSourceRequest {
            name: "Stale cached feed".to_owned(),
            category: "research".to_owned(),
            url: stale_feed.url(),
            weight: 0.75,
            enabled: true,
        })
        .await
        .expect("bridge adds source before external disable");
    for standard in after_cli_source
        .sources
        .iter()
        .filter(|source| source.origin == FfiSourceOrigin::Standard && source.enabled)
    {
        fixture.cli(&["sources", "disable", &standard.id]);
    }
    fixture.cli(&["sources", "disable", &stale_source.source.id]);
    let refresh = fixture
        .client
        .refresh("latest-source-config".to_owned(), false)
        .await
        .expect("stale bridge refresh uses current source configuration");
    assert_eq!(refresh.successful_sources, 1);
    assert_eq!(refresh.failed_sources, 0);
    assert_eq!(good_feed.request_count(), 1);
    assert_eq!(
        stale_feed.request_count(),
        0,
        "a source disabled by the CLI after bridge construction must not be contacted"
    );
    let valid_config = ConfigRepository::new(fixture.paths.clone())
        .load()
        .expect("overlapped TOML remains valid");
    assert!(
        valid_config
            .sources
            .iter()
            .any(|candidate| candidate.id == source.source.id && candidate.enabled)
    );
    assert!(
        valid_config
            .sources
            .iter()
            .any(|candidate| candidate.id == stale_source.source.id && !candidate.enabled)
    );
    let config_bytes = std::fs::read(fixture.paths.config_dir.join("config.toml"))
        .expect("read exact committed TOML bytes");
    let exact_config_revision = format!("{:x}", sha2::Sha256::digest(config_bytes));
    let after_source_refresh = fixture
        .client
        .snapshot()
        .await
        .expect("authoritative snapshot after current-config refresh");
    let after_source_revision = fixture
        .client
        .state_revision()
        .await
        .expect("authoritative revision after current-config refresh");
    assert_eq!(
        refresh.revision.source_config_revision,
        exact_config_revision
    );
    assert_eq!(after_source_refresh.revision, after_source_revision);
    assert_eq!(
        after_source_revision.source_config_revision,
        exact_config_revision
    );
    assert!(after_source_refresh.sources.iter().any(|candidate| {
        candidate.id == standard_source_id && candidate.origin == FfiSourceOrigin::Standard
    }));
    assert!(after_source_refresh.sources.iter().any(|candidate| {
        candidate.id == source.source.id && candidate.origin == FfiSourceOrigin::Personal
    }));

    fixture.cli(&[
        "models",
        "add",
        "--name",
        "Shared profile",
        "--provider",
        "open-ai",
        "--model",
        "opaque-test-model",
        "--credential-env",
        "SHARED_PROCESS_TEST_KEY",
        "--consent-provider-data-sharing",
    ]);
    fixture.cli(&["models", "use", "Shared profile"]);
    let after_cli_model = fixture
        .client
        .snapshot()
        .await
        .expect("bridge reload after CLI model");
    let profile = after_cli_model
        .model_profiles
        .iter()
        .find(|value| value.name == "Shared profile")
        .expect("CLI-created model visible to bridge");
    assert_eq!(
        after_cli_model.default_model_profile_id.as_deref(),
        Some(profile.id.as_str())
    );
    fixture
        .client
        .remove_model_profile(profile.id.clone())
        .await
        .expect("bridge removes CLI-created model");
    assert!(
        !fixture
            .cli_text(&["models", "list"])
            .contains("Shared profile")
    );

    let database_path = fixture.paths.data_dir.join("signal.sqlite3");
    let lock = rusqlite::Connection::open(&database_path).expect("open overlap barrier");
    lock.execute_batch("BEGIN IMMEDIATE")
        .expect("hold isolated SQLite write barrier");
    let mut cli_save = fixture.spawn_cli(&["save", "story-1"]);
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(
        cli_save.try_wait().expect("inspect blocked CLI").is_none(),
        "CLI writer must reach and wait behind the held SQLite barrier"
    );

    let concurrent_client = Arc::clone(&fixture.client);
    let bridge_write = tokio::spawn(async move {
        concurrent_client
            .set_story_read("story-1".to_owned(), false)
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(
        cli_save
            .try_wait()
            .expect("inspect overlapping CLI")
            .is_none()
            && !bridge_write.is_finished(),
        "separate CLI and bridge writers must demonstrably overlap before barrier release"
    );
    lock.execute_batch("COMMIT")
        .expect("release isolated SQLite write barrier");
    let bridge_result = tokio::time::timeout(std::time::Duration::from_secs(6), bridge_write)
        .await
        .expect("bridge writer completes after barrier release")
        .expect("bridge writer task remains healthy")
        .expect("bridge write after deterministic overlap");
    let cli_output = cli_save
        .wait_with_output()
        .expect("wait for overlapped CLI save");
    assert!(
        cli_output.status.success(),
        "overlapped CLI save failed: stdout={} stderr={}",
        String::from_utf8_lossy(&cli_output.stdout),
        String::from_utf8_lossy(&cli_output.stderr)
    );

    let final_snapshot = fixture
        .client
        .snapshot()
        .await
        .expect("final bridge snapshot");
    assert!(final_snapshot.latest[0].is_saved);
    assert!(!final_snapshot.latest[0].is_read);
    assert!(
        final_snapshot.revision.data_generation >= bridge_result.revision.data_generation,
        "final snapshot must include the deterministically overlapped bridge revision"
    );
    let final_revision = fixture
        .client
        .state_revision()
        .await
        .expect("final revision");
    assert!(final_revision.data_generation > read.revision.data_generation);
    assert_eq!(
        data_generation(&fixture.cli_text(&["status"])),
        final_revision.data_generation
    );
    Store::open(fixture.paths.data_dir.join("signal.sqlite3"))
        .expect("SQLite remains readable after separate-process writes")
        .status()
        .expect("SQLite status remains readable");
    let integrity = rusqlite::Connection::open(fixture.paths.data_dir.join("signal.sqlite3"))
        .expect("reopen SQLite for integrity check")
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .expect("run SQLite integrity_check");
    assert_eq!(integrity, "ok");
}
