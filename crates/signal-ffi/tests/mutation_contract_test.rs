#![cfg(feature = "test-support")]

use std::{path::PathBuf, sync::Arc};

use secrecy::SecretString;
use signal_core::{
    AppPaths, ConfigRepository, CredentialRef, CredentialStore, ProviderKind, ProviderRegistry,
    SignalApp, SignalError, Store,
    test_support::{MemoryCredentialStore, MemoryEnvironmentReader, RecordingProvider},
};
use signal_ffi::{
    AddCredentialRequest, AddFeedSourceRequest, AddModelProfileRequest, CompanionClient,
    CompanionError, FfiApiDialect, FfiCredentialDeletionStatus, FfiProfileLimitsInput,
    FfiProviderKind, FfiSourceOrigin,
};

const DELETE_DIAGNOSTIC_SENTINEL: &str = "ffi-delete-backend-diagnostic-SENTINEL";

#[derive(Default)]
struct FailingDeleteCredentialStore {
    inner: MemoryCredentialStore,
}

impl CredentialStore for FailingDeleteCredentialStore {
    fn set(&self, reference: &CredentialRef, secret: SecretString) -> signal_core::Result<()> {
        self.inner.set(reference, secret)
    }

    fn get(&self, reference: &CredentialRef) -> signal_core::Result<SecretString> {
        self.inner.get(reference)
    }

    fn delete(&self, _: &CredentialRef) -> signal_core::Result<()> {
        Err(SignalError::Credential(
            DELETE_DIAGNOSTIC_SENTINEL.to_owned(),
        ))
    }
}

struct MutationFixture {
    _root: tempfile::TempDir,
    paths: AppPaths,
    credential_store: Arc<MemoryCredentialStore>,
    environment_reader: Arc<MemoryEnvironmentReader>,
    provider: Arc<RecordingProvider>,
}

impl MutationFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary root");
        let paths = AppPaths::for_root(root.path());
        ConfigRepository::new(paths.clone())
            .load_or_create()
            .expect("fixture config");
        let store = Store::open(paths.data_dir.join("signal.sqlite3")).expect("fixture store");
        let mut briefing = signal_core::test_support::briefing_fixture();
        briefing.items[0].story.canonical_url =
            "https://stories.example.test/public?token=private-query#private-fragment".to_owned();
        store
            .commit_refresh(
                &briefing
                    .items
                    .iter()
                    .map(|item| item.story.clone())
                    .collect::<Vec<_>>(),
                &briefing,
            )
            .expect("fixture briefing");

        let credential_store = Arc::new(MemoryCredentialStore::default());
        let environment_reader = Arc::new(MemoryEnvironmentReader::default());
        let provider = Arc::new(RecordingProvider::default());
        drop(store);

        Self {
            _root: root,
            paths,
            credential_store,
            environment_reader,
            provider,
        }
    }

    fn client(&self) -> Arc<CompanionClient> {
        self.client_with_credential_store(self.credential_store.clone())
    }

    fn client_with_credential_store(
        &self,
        credential_store: Arc<dyn CredentialStore>,
    ) -> Arc<CompanionClient> {
        let mut providers = ProviderRegistry::new();
        providers.register(ProviderKind::OpenAi, self.provider.clone());
        let app = SignalApp::open_with_services(
            self.paths.clone(),
            credential_store,
            self.environment_reader.clone(),
            Arc::new(providers),
        )
        .expect("fixture app");
        CompanionClient::for_test(app)
    }

    fn sqlite_path(&self) -> PathBuf {
        self.paths.data_dir.join("signal.sqlite3")
    }

    fn config_path(&self) -> PathBuf {
        self.paths.config_dir.join("config.toml")
    }
}

fn limits() -> FfiProfileLimitsInput {
    FfiProfileLimitsInput {
        max_summaries_per_refresh: 3,
        max_daily_cost_usd: Some("1.234567".to_owned()),
        input_cost_usd_per_million: Some("0.25".to_owned()),
        output_cost_usd_per_million: Some("0.75".to_owned()),
        max_output_tokens: 256,
        timeout_seconds: 15,
        max_retries: 2,
    }
}

fn system_profile(name: &str, secret: &str) -> AddModelProfileRequest {
    AddModelProfileRequest {
        name: name.to_owned(),
        provider: FfiProviderKind::OpenAi,
        model: "gpt-fixture".to_owned(),
        endpoint: None,
        dialect: None,
        credential: AddCredentialRequest::SystemStore {
            secret: secret.to_owned(),
        },
        consent_provider_data_sharing: true,
        limits: limits(),
    }
}

#[tokio::test]
async fn story_mutations_return_the_changed_safe_story_and_new_revision() {
    // Break caught: forwarding story actions without returning the actual post-write state/revision.
    let fixture = MutationFixture::new();
    let client = fixture.client();
    let initial = client.state_revision().await.expect("initial revision");

    let saved = client
        .set_story_saved("story-1".to_owned(), true)
        .await
        .expect("save story");
    assert!(saved.story.is_saved);
    assert!(saved.revision.data_generation > initial.data_generation);

    let read = client
        .set_story_read("story-1".to_owned(), true)
        .await
        .expect("read story");
    assert!(read.story.is_read);
    assert!(read.revision.data_generation > saved.revision.data_generation);
    assert!(!read.story.canonical_url.contains('?'));
    assert!(!read.story.canonical_url.contains('#'));
}

#[tokio::test]
async fn selecting_a_variant_checks_story_membership_and_never_echoes_identifiers() {
    // Break caught: selecting a variant belonging to another story or exposing selectors in errors.
    let fixture = MutationFixture::new();
    let store = Store::open(fixture.sqlite_path()).expect("fixture store");
    let first = signal_core::test_support::summary_variant(
        "ffi-select-first",
        "ffi-select-cache-first",
        signal_core::test_support::fixed_now(),
    );
    let mut other = signal_core::test_support::summary_variant(
        "ffi-select-other",
        "ffi-select-cache-other",
        signal_core::test_support::fixed_now(),
    );
    other.story_id = "story-2".to_owned();
    store
        .upsert_stories(&[signal_core::test_support::story_fixture("story-2")])
        .expect("other story");
    store.insert_summary_variant(&first).expect("first variant");
    store.insert_summary_variant(&other).expect("other variant");
    drop(store);
    let client = fixture.client();

    let selected = client
        .select_summary_variant("story-1".to_owned(), first.id.hyphenated().to_string())
        .await
        .expect("select matching variant");
    assert_eq!(
        selected
            .story
            .selected_summary
            .as_ref()
            .map(|value| value.id.as_str()),
        Some(first.id.hyphenated().to_string().as_str())
    );

    let story_sentinel = "private-story-selector-sentinel";
    let error = client
        .select_summary_variant(story_sentinel.to_owned(), other.id.hyphenated().to_string())
        .await
        .expect_err("mismatched variant");
    assert!(matches!(
        error,
        CompanionError::NotFound | CompanionError::StorageUnavailable
    ));
    assert!(!error.to_string().contains(story_sentinel));
    assert!(
        !error
            .to_string()
            .contains(&other.id.hyphenated().to_string())
    );
}

#[tokio::test]
async fn source_mutations_use_the_config_revision_and_return_sanitized_records() {
    // Break caught: using SQLite generation for source changes or returning private URL material.
    let fixture = MutationFixture::new();
    let client = fixture.client();
    let initial = client.state_revision().await.expect("initial revision");
    let added = client
        .add_feed_source(AddFeedSourceRequest {
            name: "Personal research".to_owned(),
            category: "research".to_owned(),
            url: "https://feeds.example.test/private.xml?token=private-query#private-fragment"
                .to_owned(),
            weight: 0.7,
            enabled: true,
        })
        .await
        .expect("add source");
    assert_eq!(added.source.origin, FfiSourceOrigin::Personal);
    assert_eq!(
        added.source.feed_url,
        "https://feeds.example.test/private.xml"
    );
    assert_eq!(added.revision.data_generation, initial.data_generation);
    assert_ne!(
        added.revision.source_config_revision,
        initial.source_config_revision
    );

    let disabled = client
        .set_source_enabled(added.source.id.clone(), false)
        .await
        .expect("disable source");
    assert!(!disabled.source.enabled);
    assert_ne!(
        disabled.revision.source_config_revision,
        added.revision.source_config_revision
    );

    let removed = client
        .remove_personal_source(added.source.id)
        .await
        .expect("remove source");
    assert_eq!(removed.source.name, "Personal research");
    assert_ne!(
        removed.revision.source_config_revision,
        disabled.revision.source_config_revision
    );
}

#[test]
fn add_feed_source_request_debug_redacts_the_exact_url() {
    // Break caught: debug formatting an unvalidated source request with private URL material.
    let sentinels = [
        "source-debug-user-SENTINEL",
        "source-debug-password-SENTINEL",
        "source-debug-query-SENTINEL",
        "source-debug-fragment-SENTINEL",
    ];
    let request = AddFeedSourceRequest {
        name: "Personal research".to_owned(),
        category: "research".to_owned(),
        url: format!(
            "https://{}:{}@example.test/feed.xml?token={}#{}",
            sentinels[0], sentinels[1], sentinels[2], sentinels[3]
        ),
        weight: 0.7,
        enabled: true,
    };

    let reflected = format!("{request:?}");

    assert!(reflected.contains("<redacted>"));
    for sentinel in sentinels {
        assert!(!reflected.contains(sentinel));
    }
    assert_eq!(
        request.url,
        "https://source-debug-user-SENTINEL:source-debug-password-SENTINEL@example.test/feed.xml?token=source-debug-query-SENTINEL#source-debug-fragment-SENTINEL"
    );
}

#[tokio::test]
async fn removing_a_standard_source_is_a_safe_invalid_input_error() {
    // Break caught: allowing bundled source removal or echoing its identifier/path in the error.
    let fixture = MutationFixture::new();
    let client = fixture.client();
    let standard = client
        .snapshot()
        .await
        .expect("snapshot")
        .sources
        .into_iter()
        .find(|source| source.origin == FfiSourceOrigin::Standard)
        .expect("standard source");

    let error = client
        .remove_personal_source(standard.id.clone())
        .await
        .expect_err("standard removal");
    assert!(matches!(error, CompanionError::InvalidInput));
    assert!(!error.to_string().contains(&standard.id));
    assert!(
        !error
            .to_string()
            .contains(fixture._root.path().to_string_lossy().as_ref())
    );
}

#[tokio::test]
async fn model_profile_mutations_parse_exact_usd_and_return_safe_records() {
    // Break caught: parsing money as float or failing to return each coherent profile revision.
    let fixture = MutationFixture::new();
    let client = fixture.client();
    let added = client
        .add_model_profile(AddModelProfileRequest {
            name: "Environment profile".to_owned(),
            provider: FfiProviderKind::OpenAiCompatible,
            model: "opaque/model:beta".to_owned(),
            endpoint: Some(
                "https://provider.example.test/v1?token=private-query#private-fragment".to_owned(),
            ),
            dialect: Some(FfiApiDialect::Responses),
            credential: AddCredentialRequest::Environment {
                variable: "COMPANION_API_KEY".to_owned(),
            },
            consent_provider_data_sharing: true,
            limits: limits(),
        })
        .await
        .expect("add model profile");
    assert_eq!(
        added.profile.limits.max_daily_cost_microusd,
        Some(1_234_567)
    );
    assert_eq!(
        added.profile.limits.input_cost_microusd_per_million,
        Some(250_000)
    );
    assert_eq!(
        added.profile.limits.output_cost_microusd_per_million,
        Some(750_000)
    );
    assert_eq!(
        added.profile.endpoint.as_deref(),
        Some("https://provider.example.test/v1")
    );

    let defaulted = client
        .set_default_model_profile(added.profile.id.clone())
        .await
        .expect("set default");
    assert_eq!(defaulted.profile.id, added.profile.id);
    assert!(defaulted.revision.data_generation > added.revision.data_generation);

    let removed = client
        .remove_model_profile(added.profile.id)
        .await
        .expect("remove profile");
    assert_eq!(removed.profile.name, "Environment profile");
    assert_eq!(
        removed.credential_deletion,
        FfiCredentialDeletionStatus::NotApplicable
    );
    assert!(removed.revision.data_generation > defaulted.revision.data_generation);
}

#[tokio::test]
async fn model_removal_reports_deleted_system_credentials() {
    // Break caught: treating a successful system-store deletion as unknown or not applicable.
    let fixture = MutationFixture::new();
    let client = fixture.client();
    let added = client
        .add_model_profile(system_profile("Deleted credential", "deleted-secret"))
        .await
        .expect("add system profile");

    let removed = client
        .remove_model_profile(added.profile.id)
        .await
        .expect("remove system profile");

    assert_eq!(
        removed.credential_deletion,
        FfiCredentialDeletionStatus::Deleted
    );
    assert_eq!(fixture.credential_store.credential_count_for_test(), 0);
}

#[tokio::test]
async fn failed_credential_deletion_returns_a_safe_warning_after_profile_removal() {
    // Break caught: presenting failed Keychain cleanup as clean removal or leaking backend data.
    let fixture = MutationFixture::new();
    let credential_store = Arc::new(FailingDeleteCredentialStore::default());
    let client = fixture.client_with_credential_store(credential_store.clone());
    let secret_sentinel = "ffi-delete-secret-SENTINEL";
    let added = client
        .add_model_profile(system_profile("Deletion warning profile", secret_sentinel))
        .await
        .expect("add system profile");
    let stored_profile = Store::open(fixture.sqlite_path())
        .expect("fixture store")
        .list_model_profiles()
        .expect("stored profiles")
        .into_iter()
        .find(|profile| profile.id.hyphenated().to_string() == added.profile.id)
        .expect("stored profile");
    let credential_reference_sentinel = match stored_profile.credential {
        CredentialRef::SystemStore { account, .. } => account,
        CredentialRef::Environment { .. } => panic!("expected system-store profile"),
    };
    let selector_sentinel = added.profile.id.to_uppercase();

    let removed = client
        .remove_model_profile(selector_sentinel.clone())
        .await
        .expect("profile removal remains successful");

    assert_eq!(
        removed.credential_deletion,
        FfiCredentialDeletionStatus::DeleteFailed
    );
    assert!(removed.revision.data_generation > added.revision.data_generation);
    assert!(
        client
            .snapshot()
            .await
            .expect("post-removal snapshot")
            .model_profiles
            .is_empty()
    );
    assert_eq!(credential_store.inner.credential_count_for_test(), 1);
    let output = format!("{removed:?}");
    for private_material in [
        secret_sentinel,
        credential_reference_sentinel.as_str(),
        DELETE_DIAGNOSTIC_SENTINEL,
        selector_sentinel.as_str(),
    ] {
        assert!(
            !output.contains(private_material),
            "removal result leaked private credential cleanup material"
        );
    }
}

#[tokio::test]
async fn system_store_secret_is_write_only_and_absent_from_persistence_results_and_errors() {
    // Break caught: retaining/logging a Keychain value or persisting it in SQLite/TOML.
    let fixture = MutationFixture::new();
    let client = fixture.client();
    let secret = "ffi-system-secret-SENTINEL-do-not-leak";
    let added = client
        .add_model_profile(system_profile("Secret profile", secret))
        .await
        .expect("add system profile");

    for bytes in [
        std::fs::read(fixture.sqlite_path()).expect("SQLite bytes"),
        std::fs::read(fixture.config_path()).expect("TOML bytes"),
    ] {
        assert!(!String::from_utf8_lossy(&bytes).contains(secret));
    }
    assert!(!format!("{added:?}").contains(secret));
    assert_eq!(fixture.credential_store.credential_count_for_test(), 1);

    let invalid_secret = "ffi-invalid-money-secret-SENTINEL";
    let mut request = system_profile("Invalid money profile", invalid_secret);
    request.limits.max_daily_cost_usd = Some("0.0000001".to_owned());
    let error = client
        .add_model_profile(request)
        .await
        .expect_err("invalid exact money");
    assert!(matches!(error, CompanionError::InvalidInput));
    for display in [error.to_string(), format!("{error:?}")] {
        assert!(!display.contains(invalid_secret));
        assert!(!display.contains("model-profile/"));
        assert!(!display.contains(fixture._root.path().to_string_lossy().as_ref()));
    }
    assert_eq!(fixture.credential_store.credential_count_for_test(), 1);
}

#[tokio::test]
async fn model_test_and_story_regeneration_delegate_to_the_budgeted_provider_path() {
    // Break caught: bypassing the core test/regeneration coordinator or returning stale state.
    let fixture = MutationFixture::new();
    let client = fixture.client();
    let added = client
        .add_model_profile(system_profile("Generation profile", "generation-secret"))
        .await
        .expect("add profile");
    client
        .set_default_model_profile(added.profile.id.clone())
        .await
        .expect("set default");

    let tested = client
        .test_model_profile(added.profile.id.clone())
        .await
        .expect("test profile");
    assert_eq!(tested.profile.id, added.profile.id);
    assert!(tested.cost_may_apply);

    let regenerated = client
        .regenerate_story("story-1".to_owned(), Some(added.profile.id), true)
        .await
        .expect("regenerate story");
    assert_eq!(regenerated.story.id, "story-1");
    assert!(regenerated.story.selected_summary.is_some());
    assert!(regenerated.revision.data_generation > tested.revision.data_generation);
    assert_eq!(fixture.provider.request_count(), 2);
}

#[tokio::test]
async fn consent_credential_and_budget_failures_are_typed_and_redacted() {
    // Break caught: flattening actionable generation failures or forwarding private selectors.
    let fixture = MutationFixture::new();
    let client = fixture.client();

    let consent_secret = "ffi-consent-secret-SENTINEL";
    let mut missing_consent = system_profile("Missing consent", consent_secret);
    missing_consent.consent_provider_data_sharing = false;
    let consent_error = client
        .add_model_profile(missing_consent)
        .await
        .expect_err("consent required");
    assert!(matches!(consent_error, CompanionError::ConsentRequired));
    assert!(!format!("{consent_error:?} {consent_error}").contains(consent_secret));

    let missing_variable = "PRIVATE_MISSING_VARIABLE_SENTINEL";
    let unavailable = client
        .add_model_profile(AddModelProfileRequest {
            name: "Unavailable credential".to_owned(),
            provider: FfiProviderKind::OpenAi,
            model: "gpt-fixture".to_owned(),
            endpoint: None,
            dialect: None,
            credential: AddCredentialRequest::Environment {
                variable: missing_variable.to_owned(),
            },
            consent_provider_data_sharing: true,
            limits: limits(),
        })
        .await
        .expect("add unavailable profile");
    client
        .set_default_model_profile(unavailable.profile.id)
        .await
        .expect("set unavailable default");
    let credential_error = client
        .regenerate_story(
            "story-1".to_owned(),
            Some("Unavailable credential".to_owned()),
            true,
        )
        .await
        .expect_err("credential unavailable");
    assert!(matches!(
        credential_error,
        CompanionError::CredentialUnavailable
    ));
    assert!(!format!("{credential_error:?} {credential_error}").contains(missing_variable));

    let mut budget_request = system_profile("Budget profile", "ffi-budget-secret-SENTINEL");
    budget_request.limits.max_daily_cost_usd = Some("0.000001".to_owned());
    budget_request.limits.input_cost_usd_per_million = Some("1".to_owned());
    budget_request.limits.output_cost_usd_per_million = Some("1".to_owned());
    let budget = client
        .add_model_profile(budget_request)
        .await
        .expect("add budget profile");
    client
        .set_default_model_profile(budget.profile.id)
        .await
        .expect("set budget default");
    let budget_error = client
        .regenerate_story(
            "story-1".to_owned(),
            Some("Budget profile".to_owned()),
            true,
        )
        .await
        .expect_err("budget exhausted");
    assert!(matches!(budget_error, CompanionError::BudgetExhausted));
    assert!(!format!("{budget_error:?} {budget_error}").contains("Budget profile"));
}
