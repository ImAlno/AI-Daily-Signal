use std::sync::{Arc, Mutex};

use secrecy::{ExposeSecret, SecretString};
use signal_core::{
    CredentialRef, CredentialResolver, CredentialStore, EnvironmentReader, ResolvedCredential,
    Result, SignalError, persist_system_credential_then,
};

#[test]
fn profiles_resolve_separate_system_credentials() {
    let vault = signal_core::test_support::MemoryCredentialStore::default();
    let first = CredentialRef::for_profile(uuid::Uuid::new_v4());
    let second = CredentialRef::for_profile(uuid::Uuid::new_v4());

    vault
        .set(&first, SecretString::from("alpha".to_owned()))
        .unwrap();
    vault
        .set(&second, SecretString::from("beta".to_owned()))
        .unwrap();

    assert_eq!(vault.expose_for_test(&first), "alpha");
    assert_eq!(vault.expose_for_test(&second), "beta");
}

#[test]
fn sentinel_secret_never_appears_in_debug_or_serialized_reference() {
    let secret = "SENTINEL-DO-NOT-LEAK";
    let resolved = ResolvedCredential::new(secret.to_owned());

    assert!(!format!("{resolved:?}").contains(secret));
    assert!(
        !serde_json::to_string(&CredentialRef::for_profile(uuid::Uuid::new_v4()))
            .unwrap()
            .contains(secret)
    );
}

#[test]
fn missing_environment_credential_is_reported_without_a_system_store_lookup() {
    let store = TrackingStore::default();
    let environment = FixedEnvironment::missing();
    let reference = CredentialRef::Environment {
        variable: "MISSING_API_KEY".to_owned(),
    };

    let error = CredentialResolver::new(&store, &environment)
        .resolve(&reference)
        .unwrap_err();

    assert_eq!(error.to_string(), "credential is missing");
    assert_eq!(store.get_calls(), 0);
}

#[test]
fn empty_environment_credential_is_rejected() {
    let store = TrackingStore::default();
    let environment = FixedEnvironment::empty();
    let reference = CredentialRef::Environment {
        variable: "EMPTY_API_KEY".to_owned(),
    };

    let error = CredentialResolver::new(&store, &environment)
        .resolve(&reference)
        .unwrap_err();

    assert_eq!(error.to_string(), "credential is empty");
}

#[test]
fn empty_and_whitespace_system_credentials_are_rejected_after_store_read() {
    for value in ["", " \t\n"] {
        let store = signal_core::test_support::MemoryCredentialStore::default();
        let environment = FixedEnvironment::missing();
        let reference = CredentialRef::for_profile(uuid::Uuid::new_v4());
        store
            .set(&reference, SecretString::from(value.to_owned()))
            .unwrap();

        let error = CredentialResolver::new(&store, &environment)
            .resolve(&reference)
            .unwrap_err();

        assert_eq!(error.to_string(), "credential is empty");
        if !value.is_empty() {
            assert!(!error.to_string().contains(value));
        }
    }
}

#[test]
fn non_unicode_environment_credential_is_reported_without_exposing_its_value() {
    let store = TrackingStore::default();
    let environment = FixedEnvironment::non_unicode();
    let reference = CredentialRef::Environment {
        variable: "NON_UNICODE_API_KEY".to_owned(),
    };

    let error = CredentialResolver::new(&store, &environment)
        .resolve(&reference)
        .unwrap_err();

    assert_eq!(error.to_string(), "credential is unavailable");
    assert!(!error.to_string().contains("SENTINEL-DO-NOT-LEAK"));
}

#[test]
fn present_environment_credential_resolves_without_a_system_store_lookup() {
    let store = TrackingStore::default();
    let environment = FixedEnvironment::present("environment-secret");
    let reference = CredentialRef::Environment {
        variable: "PRESENT_API_KEY".to_owned(),
    };

    let resolved = CredentialResolver::new(&store, &environment)
        .resolve(&reference)
        .unwrap();

    assert!(!format!("{resolved:?}").contains("environment-secret"));
    assert_eq!(store.get_calls(), 0);
}

#[test]
fn missing_system_credential_does_not_fall_through_to_environment() {
    let store = TrackingStore::default();
    let environment = FixedEnvironment::present("environment-secret");
    let reference = CredentialRef::for_profile(uuid::Uuid::new_v4());

    let error = CredentialResolver::new(&store, &environment)
        .resolve(&reference)
        .unwrap_err();

    assert_eq!(error.to_string(), "credential is missing");
    assert_eq!(environment.read_calls(), 0);
}

#[test]
fn system_store_failure_is_redacted() {
    let store = LeakyStore;
    let environment = FixedEnvironment::missing();
    let reference = CredentialRef::for_profile(uuid::Uuid::new_v4());

    let error = CredentialResolver::new(&store, &environment)
        .resolve(&reference)
        .unwrap_err();

    assert_eq!(error.to_string(), "credential is unavailable");
    assert!(!error.to_string().contains("SENTINEL-DO-NOT-LEAK"));
}

#[test]
fn failed_nonsecret_persistence_deletes_an_initially_missing_credential() {
    let store = signal_core::test_support::MemoryCredentialStore::default();
    let reference = CredentialRef::for_profile(uuid::Uuid::new_v4());

    let error = persist_system_credential_then(
        &store,
        &reference,
        SecretString::from("temporary-secret".to_owned()),
        || Err::<(), _>(SignalError::Storage("database write failed".to_owned())),
    )
    .unwrap_err();

    assert_eq!(error.to_string(), "storage error: database write failed");
    assert_eq!(
        store.get(&reference).unwrap_err().to_string(),
        "credential is missing"
    );
}

#[test]
fn failed_nonsecret_persistence_restores_an_existing_credential() {
    let store = signal_core::test_support::MemoryCredentialStore::default();
    let reference = CredentialRef::for_profile(uuid::Uuid::new_v4());
    store
        .set(&reference, SecretString::from("existing-secret".to_owned()))
        .unwrap();

    let error = persist_system_credential_then(
        &store,
        &reference,
        SecretString::from("replacement-secret".to_owned()),
        || Err::<(), _>(SignalError::Storage("database write failed".to_owned())),
    )
    .unwrap_err();

    assert_eq!(error.to_string(), "storage error: database write failed");
    let stored = store
        .get(&reference)
        .ok()
        .map(|secret| secret.expose_secret().to_owned());
    assert_eq!(stored.as_deref(), Some("existing-secret"));
}

#[derive(Default)]
struct TrackingStore {
    get_calls: Mutex<u32>,
}

impl TrackingStore {
    fn get_calls(&self) -> u32 {
        *self.get_calls.lock().unwrap()
    }
}

impl CredentialStore for TrackingStore {
    fn set(&self, _: &CredentialRef, _: SecretString) -> Result<()> {
        Ok(())
    }

    fn get(&self, _: &CredentialRef) -> Result<SecretString> {
        *self.get_calls.lock().unwrap() += 1;
        Err(SignalError::Credential("credential is missing".to_owned()))
    }

    fn delete(&self, _: &CredentialRef) -> Result<()> {
        Ok(())
    }
}

struct LeakyStore;

impl CredentialStore for LeakyStore {
    fn set(&self, _: &CredentialRef, _: SecretString) -> Result<()> {
        Ok(())
    }

    fn get(&self, _: &CredentialRef) -> Result<SecretString> {
        Err(SignalError::Credential("SENTINEL-DO-NOT-LEAK".to_owned()))
    }

    fn delete(&self, _: &CredentialRef) -> Result<()> {
        Ok(())
    }
}

enum EnvironmentValue {
    Missing,
    Empty,
    NonUnicode,
    Present(String),
}

struct FixedEnvironment {
    value: EnvironmentValue,
    read_calls: Arc<Mutex<u32>>,
}

impl FixedEnvironment {
    fn missing() -> Self {
        Self::new(EnvironmentValue::Missing)
    }

    fn empty() -> Self {
        Self::new(EnvironmentValue::Empty)
    }

    fn non_unicode() -> Self {
        Self::new(EnvironmentValue::NonUnicode)
    }

    fn present(value: &str) -> Self {
        Self::new(EnvironmentValue::Present(value.to_owned()))
    }

    fn new(value: EnvironmentValue) -> Self {
        Self {
            value,
            read_calls: Arc::new(Mutex::new(0)),
        }
    }

    fn read_calls(&self) -> u32 {
        *self.read_calls.lock().unwrap()
    }
}

impl EnvironmentReader for FixedEnvironment {
    fn read(&self, _: &str) -> Result<Option<String>> {
        *self.read_calls.lock().unwrap() += 1;
        match &self.value {
            EnvironmentValue::Missing => Ok(None),
            EnvironmentValue::Empty => Ok(Some(String::new())),
            EnvironmentValue::NonUnicode => {
                Err(SignalError::Credential("SENTINEL-DO-NOT-LEAK".to_owned()))
            }
            EnvironmentValue::Present(value) => Ok(Some(value.clone())),
        }
    }
}
