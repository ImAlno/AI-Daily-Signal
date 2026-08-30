#![cfg(feature = "test-support")]

use std::sync::Arc;

use secrecy::SecretString;
use signal_core::{
    ApiDialect, CredentialRef, CredentialStore, ProviderFailure, ProviderKind, ProviderRegistry,
    RequestChargeStatus, SignalApp, SignalError, SourceKind, Store,
    test_support::{MemoryCredentialStore, MemoryEnvironmentReader},
};
use signal_ffi::{CompanionClient, CompanionError, FfiCollectionState, FfiCredentialSourceKind};

mod test_support {
    use super::*;
    use signal_core::{AppPaths, ConfigRepository};

    pub struct CompanionAppFixture {
        _root: tempfile::TempDir,
        paths: AppPaths,
        credential_store: Arc<MemoryCredentialStore>,
        credential_value: String,
        credential_account: String,
        provider_body: String,
        feed_url: String,
        provider_endpoint: String,
    }

    impl CompanionAppFixture {
        pub fn open_app(&self) -> SignalApp {
            SignalApp::open_with_services(
                self.paths.clone(),
                self.credential_store.clone(),
                Arc::new(MemoryEnvironmentReader::default()),
                Arc::new(ProviderRegistry::new()),
            )
            .expect("fixture app")
        }

        pub fn credential_sentinel(&self) -> &str {
            &self.credential_value
        }

        pub fn credential_reference_sentinel(&self) -> &str {
            &self.credential_account
        }

        pub fn provider_body_sentinel(&self) -> &str {
            &self.provider_body
        }

        pub fn temporary_root_sentinel(&self) -> &str {
            self._root.path().to_str().expect("UTF-8 temporary root")
        }

        pub fn feed_url_sentinel(&self) -> &str {
            &self.feed_url
        }

        pub fn provider_endpoint_sentinel(&self) -> &str {
            &self.provider_endpoint
        }

        pub fn stored_feed_url(&self) -> String {
            let config = ConfigRepository::new(self.paths.clone())
                .load()
                .expect("stored config");
            match &config.sources[0].kind {
                SourceKind::Feed { url } => url.clone(),
            }
        }

        pub fn stored_provider_endpoint(&self) -> String {
            self.open_app().list_models().expect("stored models")[0]
                .endpoint
                .as_ref()
                .expect("stored endpoint")
                .to_string()
        }

        pub fn save_story_in_another_app(&self) {
            self.open_app()
                .set_saved("story-1", true)
                .expect("save story");
        }

        pub fn disable_source_in_another_app(&self) {
            self.open_app()
                .set_source_enabled("primary", false)
                .expect("disable source");
        }

        pub fn remove_profile_credential(&self) {
            let profile = self
                .open_app()
                .list_models()
                .expect("stored models")
                .remove(0);
            self.credential_store
                .delete(&profile.credential)
                .expect("remove fixture credential");
        }
    }

    pub fn companion_app_fixture() -> CompanionAppFixture {
        let root = tempfile::tempdir().expect("temporary fixture root");
        let paths = AppPaths::for_root(root.path());
        let feed_url = "https://feeds.example.test/private/path?feed_query_name_sentinel=feed_query_value_sentinel#feed_fragment_sentinel".to_owned();
        let mut config = signal_core::test_support::config_fixture();
        match &mut config.sources[0].kind {
            SourceKind::Feed { url } => *url = feed_url.clone(),
        }
        ConfigRepository::new(paths.clone())
            .save(&config)
            .expect("fixture config");

        let store = Store::open(paths.data_dir.join("signal.sqlite3")).expect("fixture store");
        let provider_endpoint = "https://provider.example.test/v1?endpoint_query_name_sentinel=endpoint_query_value_sentinel#endpoint_fragment_sentinel".to_owned();
        let mut profile =
            signal_core::test_support::model_profile("Companion", ProviderKind::OpenAiCompatible);
        profile.model = "companion-model".to_owned();
        profile.endpoint = Some(provider_endpoint.parse().expect("fixture endpoint"));
        profile.dialect = Some(ApiDialect::Responses);
        let credential_account = match &profile.credential {
            CredentialRef::SystemStore { account, .. } => account.clone(),
            CredentialRef::Environment { .. } => panic!("fixture must use the system store"),
        };
        store.create_model_profile(&profile).expect("model profile");
        store
            .set_default_model_profile(Some(profile.id))
            .expect("default model profile");

        let credential_value = "ffi-credential-value-sentinel".to_owned();
        let credential_store = Arc::new(MemoryCredentialStore::default());
        credential_store
            .set(
                &profile.credential,
                SecretString::from(credential_value.clone()),
            )
            .expect("fixture credential");

        let mut briefing = signal_core::test_support::briefing_fixture();
        briefing.items[0].story.is_saved = true;
        let mut summary = signal_core::test_support::summary_variant(
            "ffi-summary",
            "ffi-cache-key",
            signal_core::test_support::fixed_now(),
        );
        summary.profile_id = Some(profile.id);
        summary.model = profile.model.clone();
        briefing.items[0].selected_summary = Some(summary.clone());
        store
            .commit_refresh_with_counts_and_variants(
                &[briefing.items[0].story.clone()],
                &briefing,
                &[summary],
                3,
                1,
            )
            .expect("fixture refresh");

        CompanionAppFixture {
            _root: root,
            paths,
            credential_store,
            credential_value,
            credential_account,
            provider_body: "ffi-provider-body-sentinel".to_owned(),
            feed_url,
            provider_endpoint,
        }
    }

    pub fn empty_companion_app_fixture() -> CompanionAppFixture {
        let root = tempfile::tempdir().expect("temporary fixture root");
        let paths = AppPaths::for_root(root.path());
        ConfigRepository::new(paths.clone())
            .save(&signal_core::test_support::config_fixture())
            .expect("fixture config");
        Store::open(paths.data_dir.join("signal.sqlite3")).expect("fixture store");
        CompanionAppFixture {
            _root: root,
            paths,
            credential_store: Arc::new(MemoryCredentialStore::default()),
            credential_value: "empty-credential-sentinel".to_owned(),
            credential_account: "empty-account-sentinel".to_owned(),
            provider_body: "empty-provider-body-sentinel".to_owned(),
            feed_url: "https://empty.example.test/feed".to_owned(),
            provider_endpoint: "https://empty.example.test/v1".to_owned(),
        }
    }
}

#[tokio::test]
async fn snapshot_projects_urls_without_query_userinfo_or_fragment_material() {
    let fixture = test_support::companion_app_fixture();
    let snapshot = CompanionClient::for_test(fixture.open_app())
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(
        snapshot.sources[0].feed_url,
        "https://feeds.example.test/private/path"
    );
    assert_eq!(
        snapshot.model_profiles[0].endpoint.as_deref(),
        Some("https://provider.example.test/v1")
    );

    let exported = format!("{snapshot:?}");
    for private_material in [
        fixture.feed_url_sentinel(),
        fixture.provider_endpoint_sentinel(),
        "feed_query_name_sentinel",
        "feed_query_value_sentinel",
        "feed_fragment_sentinel",
        "endpoint_query_name_sentinel",
        "endpoint_query_value_sentinel",
        "endpoint_fragment_sentinel",
    ] {
        assert!(
            !exported.contains(private_material),
            "snapshot leaked private URL material"
        );
    }

    assert_eq!(fixture.stored_feed_url(), fixture.feed_url_sentinel());
    assert_eq!(
        fixture.stored_provider_endpoint(),
        fixture.provider_endpoint_sentinel()
    );
}

#[tokio::test]
async fn snapshot_maps_core_state_without_private_material() {
    let fixture = test_support::companion_app_fixture();
    let client = CompanionClient::for_test(fixture.open_app());

    let snapshot = client.snapshot().await.expect("snapshot");

    assert!(snapshot.revision.data_generation > 0);
    assert_eq!(snapshot.revision.source_config_revision.len(), 64);
    assert_eq!(snapshot.status.state, FfiCollectionState::Ready);
    assert_eq!(snapshot.status.refresh.as_ref().unwrap().story_count, 1);
    assert_eq!(
        snapshot.status.refresh.as_ref().unwrap().last_refresh_at,
        "2026-08-29T09:30:00+00:00"
    );

    let today = snapshot.today.as_ref().expect("today briefing");
    assert_eq!(today.date, "2026-08-29");
    assert_eq!(today.generated_at, "2026-08-29T09:30:00+00:00");
    assert!(today.is_stale);
    let item = &today.items[0];
    assert_eq!(item.position, 1);
    assert_eq!(item.section, "top_signals");
    assert_eq!(item.story.title, "A deterministic signal");
    assert_eq!(item.story.score.total, 76.5);
    let summary = item.selected_summary.as_ref().expect("selected summary");
    assert_eq!(
        summary.fields.what_happened,
        "A deterministic event happened."
    );
    assert_eq!(summary.model, "companion-model");
    assert_eq!(item.summary_variants, vec![summary.clone()]);

    assert_eq!(snapshot.latest[0].id, "story-1");
    assert_eq!(
        snapshot.latest[0]
            .selected_summary
            .as_ref()
            .expect("latest selected summary")
            .id,
        summary.id
    );
    assert_eq!(snapshot.latest[0].summary_variants, vec![summary.clone()]);
    assert_eq!(snapshot.saved[0].id, "story-1");
    assert_eq!(snapshot.saved[0].summary_variants, vec![summary.clone()]);
    assert_eq!(snapshot.sources.len(), 4);
    assert_eq!(snapshot.sources[0].id, "primary");
    assert_eq!(snapshot.model_profiles.len(), 1);
    assert_eq!(
        snapshot.model_profiles[0].credential_source,
        FfiCredentialSourceKind::SystemStore
    );
    assert_eq!(
        snapshot.default_model_profile_id.as_deref(),
        Some(snapshot.model_profiles[0].id.as_str())
    );
    assert!(snapshot.has_usable_ai_profile);

    let debug_output = format!("{snapshot:?}");
    for sentinel in [
        fixture.credential_sentinel(),
        fixture.credential_reference_sentinel(),
        fixture.provider_body_sentinel(),
        fixture.temporary_root_sentinel(),
    ] {
        assert!(
            !debug_output.contains(sentinel),
            "snapshot leaked private material"
        );
    }
}

#[test]
fn typed_errors_discard_private_core_details() {
    let fixture = test_support::companion_app_fixture();
    let private_errors = [
        SignalError::Credential(fixture.credential_sentinel().to_owned()),
        SignalError::NotFound(fixture.credential_reference_sentinel().to_owned()),
        SignalError::Provider(ProviderFailure::for_test(
            fixture.credential_sentinel(),
            fixture.provider_body_sentinel(),
        )),
        SignalError::Io(std::io::Error::other(
            fixture.temporary_root_sentinel().to_owned(),
        )),
    ];

    for error in private_errors {
        let redacted = CompanionError::from(error);
        let output = format!("{redacted:?} {redacted}");
        for sentinel in [
            fixture.credential_sentinel(),
            fixture.credential_reference_sentinel(),
            fixture.provider_body_sentinel(),
            fixture.temporary_root_sentinel(),
        ] {
            assert!(!output.contains(sentinel), "error leaked private material");
        }
    }

    let provider_error = CompanionError::from(SignalError::Provider(ProviderFailure::new(
        signal_core::ProviderFailureKind::ProviderUnavailable,
        RequestChargeStatus::PossiblySent,
    )));
    assert!(matches!(
        provider_error,
        CompanionError::ProviderUnavailable
    ));
}

#[tokio::test]
async fn empty_store_maps_to_not_initialized_without_a_today_error() {
    let fixture = test_support::empty_companion_app_fixture();
    let snapshot = CompanionClient::for_test(fixture.open_app())
        .snapshot()
        .await
        .expect("empty snapshot");

    assert_eq!(snapshot.status.state, FfiCollectionState::NotInitialized);
    assert!(snapshot.status.refresh.is_none());
    assert!(snapshot.today.is_none());
    assert!(snapshot.latest.is_empty());
    assert!(snapshot.saved.is_empty());
    assert!(snapshot.model_profiles.is_empty());
    assert_eq!(snapshot.default_model_profile_id, None);
    assert!(!snapshot.has_usable_ai_profile);
}

#[tokio::test]
async fn snapshot_does_not_mark_a_missing_credential_profile_as_usable() {
    let fixture = test_support::companion_app_fixture();
    fixture.remove_profile_credential();

    let snapshot = CompanionClient::for_test(fixture.open_app())
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(snapshot.model_profiles.len(), 1);
    assert!(!snapshot.has_usable_ai_profile);
}

#[tokio::test]
async fn state_revision_preserves_database_and_source_config_components() {
    let fixture = test_support::companion_app_fixture();
    let client = CompanionClient::for_test(fixture.open_app());
    let initial = client.state_revision().await.expect("initial revision");

    fixture.save_story_in_another_app();
    let database_change = client.state_revision().await.expect("database revision");
    assert!(database_change.data_generation > initial.data_generation);
    assert_eq!(
        database_change.source_config_revision,
        initial.source_config_revision
    );

    fixture.disable_source_in_another_app();
    let config_change = client.state_revision().await.expect("config revision");
    assert_eq!(
        config_change.data_generation,
        database_change.data_generation
    );
    assert_ne!(
        config_change.source_config_revision,
        database_change.source_config_revision
    );
}
