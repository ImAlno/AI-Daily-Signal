#![cfg(feature = "test-support")]

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
};

use chrono::Utc;
use signal_core::{
    AppPaths, ConfigRepository, ProviderRegistry, SignalApp, Store,
    test_support::{MemoryCredentialStore, MemoryEnvironmentReader},
};
use signal_ffi::{AddFeedSourceRequest, CompanionClient, FfiSourceOrigin};

struct SharedProcessFixture {
    _root: tempfile::TempDir,
    paths: AppPaths,
    client: Arc<CompanionClient>,
}

impl SharedProcessFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("isolated application root");
        let paths = AppPaths::for_root(root.path());
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
            _root: root,
            paths,
            client,
        }
    }

    fn root(&self) -> &std::path::Path {
        self._root.path()
    }

    fn cli(&self, args: &[&str]) -> std::process::Output {
        let output = Command::new(cli_binary())
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
        Command::new(cli_binary())
            .env("SIGNAL_HOME", self.root())
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn real signal CLI process")
    }
}

fn cli_binary() -> PathBuf {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .to_path_buf();
    let binary = workspace
        .join("target")
        .join("debug")
        .join(if cfg!(windows) {
            "signal.exe"
        } else {
            "signal"
        });
    assert!(
        binary.is_file(),
        "build the real CLI first with `cargo build -p signal-cli`: {}",
        binary.display()
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

#[tokio::test]
async fn bridge_and_real_cli_share_one_uncorrupted_application_root() {
    // Break caught: app and CLI drifting into separate stores, stale config, or unsafe concurrent
    // writes despite resolving one explicit application root.
    let fixture = SharedProcessFixture::new();
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

    let source = fixture
        .client
        .add_feed_source(AddFeedSourceRequest {
            name: "Shared process feed".to_owned(),
            category: "research".to_owned(),
            url: "https://shared.example.test/feed.xml".to_owned(),
            weight: 0.75,
            enabled: true,
        })
        .await
        .expect("bridge adds source");
    assert_eq!(source.source.origin, FfiSourceOrigin::Personal);
    assert!(
        fixture
            .cli_text(&["sources", "list"])
            .contains("Shared process feed")
    );
    fixture.cli(&["sources", "disable", &source.source.id]);
    let after_cli_source = fixture
        .client
        .snapshot()
        .await
        .expect("bridge reload after CLI source");
    assert!(
        after_cli_source
            .sources
            .iter()
            .any(|value| value.id == source.source.id && !value.enabled)
    );
    assert_ne!(
        after_cli_source.revision.source_config_revision,
        initial.source_config_revision
    );

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

    let mut cli_save = fixture.spawn_cli(&["save", "story-1"]);
    fixture
        .client
        .set_story_read("story-1".to_owned(), false)
        .await
        .expect("bridge write concurrent with CLI process");
    assert!(cli_save.wait().expect("wait for CLI save").success());

    let final_snapshot = fixture
        .client
        .snapshot()
        .await
        .expect("final bridge snapshot");
    assert!(final_snapshot.latest[0].is_saved);
    assert!(!final_snapshot.latest[0].is_read);
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
}
