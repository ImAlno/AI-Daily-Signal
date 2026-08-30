use std::sync::Arc;

use sha2::Digest;
use signal_core::{
    AppPaths, ConfigRepository, ProviderRegistry, RefreshOptions, SignalApp, SignalError, Store,
    test_support::{MemoryCredentialStore, MemoryEnvironmentReader},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>Current config fixture</title>
  <item><guid>current-config-story</guid><title>Current config story</title>
    <link>https://example.test/current-config-story</link>
    <description>A deterministic source configuration fixture.</description></item>
</channel></rss>"#;

struct CompanionFixture {
    _root: tempfile::TempDir,
    paths: AppPaths,
}

impl CompanionFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(root.path());
        ConfigRepository::new(paths.clone())
            .save(&signal_core::test_support::config_fixture())
            .unwrap();

        let store = Store::open(paths.data_dir.join("signal.sqlite3")).unwrap();
        let briefing = signal_core::test_support::briefing_fixture();
        store
            .commit_refresh(
                &briefing
                    .items
                    .iter()
                    .map(|item| item.story.clone())
                    .collect::<Vec<_>>(),
                &briefing,
            )
            .unwrap();

        Self { _root: root, paths }
    }

    fn open_app(&self) -> SignalApp {
        SignalApp::open_with_services(
            self.paths.clone(),
            Arc::new(MemoryCredentialStore::default()),
            Arc::new(MemoryEnvironmentReader::default()),
            Arc::new(ProviderRegistry::new()),
        )
        .unwrap()
    }

    fn store(&self) -> Store {
        Store::open(self.paths.data_dir.join("signal.sqlite3")).unwrap()
    }

    fn disable_source_in_a_separate_app(&self, source_id: &str) {
        self.open_app()
            .set_source_enabled(source_id, false)
            .unwrap();
    }
}

#[test]
fn state_revision_detects_database_and_external_source_config_changes() {
    let fixture = CompanionFixture::new();
    let mut app = fixture.open_app();
    let initial = app.state_revision().unwrap();

    app.set_saved("story-1", true).unwrap();
    let database_change = app.state_revision().unwrap();
    assert!(database_change.data_generation > initial.data_generation);

    fixture.disable_source_in_a_separate_app("primary");
    let config_change = app.state_revision().unwrap();
    assert_ne!(
        config_change.source_config_revision,
        initial.source_config_revision
    );
}

#[test]
fn source_config_revision_is_the_lowercase_sha256_of_stored_toml_bytes() {
    let fixture = CompanionFixture::new();
    let bytes = std::fs::read(fixture.paths.config_dir.join("config.toml")).unwrap();

    assert_eq!(
        ConfigRepository::new(fixture.paths).revision().unwrap(),
        format!("{:x}", sha2::Sha256::digest(bytes))
    );
}

#[tokio::test]
async fn refresh_uses_current_source_configuration_after_external_edit() {
    // Break caught: refresh contacting a feed that another app disabled after this app opened.
    let fixture = CompanionFixture::new();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/feed.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FEED))
        .mount(&server)
        .await;
    let repository = ConfigRepository::new(fixture.paths.clone());
    let mut config = repository.load().unwrap();
    config.sources = vec![signal_core::Source {
        id: "primary".to_owned(),
        name: "Current config source".to_owned(),
        category: "research".to_owned(),
        enabled: true,
        weight: 1.0,
        kind: signal_core::SourceKind::Feed {
            url: format!("{}/feed.xml", server.uri()),
        },
    }];
    repository.save(&config).unwrap();
    let stale = fixture.open_app();

    fixture.disable_source_in_a_separate_app("primary");
    let error = stale
        .refresh_with_options(
            signal_core::test_support::fixed_now(),
            RefreshOptions { ai: false },
        )
        .await
        .unwrap_err();

    assert!(matches!(error, SignalError::InvalidConfiguration(_)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[test]
fn story_actions_and_summary_variants_return_updated_core_values() {
    let fixture = CompanionFixture::new();
    let store = fixture.store();
    let first_variant = signal_core::test_support::summary_variant(
        "first-variant",
        "first-cache-key",
        signal_core::test_support::fixed_now(),
    );
    let mut other_variant = signal_core::test_support::summary_variant(
        "other-variant",
        "other-cache-key",
        signal_core::test_support::fixed_now(),
    );
    other_variant.story_id = "story-2".to_owned();
    store
        .upsert_stories(&[signal_core::test_support::story_fixture("story-2")])
        .unwrap();
    store.insert_summary_variant(&first_variant).unwrap();
    store.insert_summary_variant(&other_variant).unwrap();

    let app = fixture.open_app();
    assert!(app.set_saved("story-1", true).unwrap().is_saved);
    assert!(app.set_read("story-1", true).unwrap().is_read);
    assert_eq!(
        app.summary_variants("story-1").unwrap(),
        vec![first_variant.clone()]
    );
    assert_eq!(
        app.select_summary_variant("story-1", first_variant.id)
            .unwrap(),
        first_variant
    );
    assert!(
        app.select_summary_variant("story-1", other_variant.id)
            .is_err()
    );
}
