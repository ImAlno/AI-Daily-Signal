use std::sync::Arc;

use signal_core::{
    AppPaths, ConfigRepository, CredentialRef, ProviderKind, ProviderRegistry, SignalApp, Store,
    test_support::{MemoryCredentialStore, MemoryEnvironmentReader},
};

struct AvailabilityFixture {
    _root: tempfile::TempDir,
    paths: AppPaths,
}

impl AvailabilityFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary fixture root");
        let paths = AppPaths::for_root(root.path());
        ConfigRepository::new(paths.clone())
            .save(&signal_core::test_support::config_fixture())
            .expect("fixture config");
        Store::open(paths.data_dir.join("signal.sqlite3")).expect("fixture store");
        Self { _root: root, paths }
    }

    fn store(&self) -> Store {
        Store::open(self.paths.data_dir.join("signal.sqlite3")).expect("fixture store")
    }

    fn open_app(
        &self,
        credentials: Arc<MemoryCredentialStore>,
        environment: Arc<MemoryEnvironmentReader>,
    ) -> SignalApp {
        SignalApp::open_with_services(
            self.paths.clone(),
            credentials,
            environment,
            Arc::new(ProviderRegistry::new()),
        )
        .expect("fixture app")
    }
}

#[test]
fn missing_system_store_and_environment_credentials_are_not_usable() {
    let fixture = AvailabilityFixture::new();
    let system = signal_core::test_support::model_profile("Missing system", ProviderKind::OpenAi);
    let mut environment =
        signal_core::test_support::model_profile("Missing environment", ProviderKind::Gemini);
    environment.credential = CredentialRef::Environment {
        variable: "MISSING_ENVIRONMENT_CREDENTIAL".to_owned(),
    };
    fixture
        .store()
        .create_model_profile(&system)
        .expect("system profile");
    fixture
        .store()
        .create_model_profile(&environment)
        .expect("environment profile");
    let app = fixture.open_app(
        Arc::new(MemoryCredentialStore::default()),
        Arc::new(MemoryEnvironmentReader::default()),
    );

    assert!(!app.has_usable_ai_profile().expect("availability"));
}

#[test]
fn resolvable_environment_credential_is_usable_without_a_provider_call() {
    let fixture = AvailabilityFixture::new();
    let mut profile =
        signal_core::test_support::model_profile("Available environment", ProviderKind::Gemini);
    profile.credential = CredentialRef::Environment {
        variable: "AVAILABLE_ENVIRONMENT_CREDENTIAL".to_owned(),
    };
    fixture
        .store()
        .create_model_profile(&profile)
        .expect("environment profile");
    let environment = Arc::new(MemoryEnvironmentReader::default());
    environment.set(
        "AVAILABLE_ENVIRONMENT_CREDENTIAL",
        Some("deterministic-safe-test-credential".to_owned()),
    );
    let app = fixture.open_app(Arc::new(MemoryCredentialStore::default()), environment);

    assert!(app.has_usable_ai_profile().expect("availability"));
}
