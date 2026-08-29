use std::fmt;

use keyring::v1::{Entry, Error as KeyringError};
use secrecy::{ExposeSecret, SecretString};

use crate::{CredentialRef, Result, SignalError};

const MISSING_CREDENTIAL: &str = "credential is missing";
const EMPTY_CREDENTIAL: &str = "credential is empty";
const UNAVAILABLE_CREDENTIAL: &str = "credential is unavailable";
const STORE_FAILURE: &str = "credential store operation failed";

pub trait CredentialStore: Send + Sync {
    fn set(&self, reference: &CredentialRef, secret: SecretString) -> Result<()>;
    fn get(&self, reference: &CredentialRef) -> Result<SecretString>;
    fn delete(&self, reference: &CredentialRef) -> Result<()>;
}

pub trait EnvironmentReader: Send + Sync {
    fn read(&self, variable: &str) -> Result<Option<String>>;
}

#[derive(Default)]
pub struct SystemCredentialStore;

impl CredentialStore for SystemCredentialStore {
    fn set(&self, reference: &CredentialRef, secret: SecretString) -> Result<()> {
        entry_for(reference)?
            .set_password(secret.expose_secret())
            .map_err(map_keyring_error)
    }

    fn get(&self, reference: &CredentialRef) -> Result<SecretString> {
        entry_for(reference)?
            .get_password()
            .map(SecretString::from)
            .map_err(map_keyring_error)
    }

    fn delete(&self, reference: &CredentialRef) -> Result<()> {
        match entry_for(reference)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error(error)),
        }
    }
}

#[derive(Default)]
pub struct ProcessEnvironmentReader;

impl EnvironmentReader for ProcessEnvironmentReader {
    fn read(&self, variable: &str) -> Result<Option<String>> {
        match std::env::var_os(variable) {
            None => Ok(None),
            Some(value) => value.into_string().map(Some).map_err(|_| unavailable()),
        }
    }
}

pub struct CredentialResolver<'a> {
    system_store: &'a dyn CredentialStore,
    environment: &'a dyn EnvironmentReader,
}

impl<'a> CredentialResolver<'a> {
    pub fn new(
        system_store: &'a dyn CredentialStore,
        environment: &'a dyn EnvironmentReader,
    ) -> Self {
        Self {
            system_store,
            environment,
        }
    }

    pub fn resolve(&self, reference: &CredentialRef) -> Result<ResolvedCredential> {
        match reference {
            CredentialRef::SystemStore { .. } => self
                .system_store
                .get(reference)
                .map_err(redact_store_error)
                .map(ResolvedCredential),
            CredentialRef::Environment { variable } => self
                .environment
                .read(variable)
                .map_err(|_| unavailable())?
                .ok_or_else(missing)
                .and_then(nonempty_credential)
                .map(ResolvedCredential),
        }
    }
}

pub struct ResolvedCredential(#[allow(dead_code)] SecretString);

impl ResolvedCredential {
    pub fn new(secret: String) -> Self {
        Self(SecretString::from(secret))
    }

    #[allow(dead_code)]
    pub(crate) fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for ResolvedCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ResolvedCredential([REDACTED])")
    }
}

pub fn persist_system_credential_then<T>(
    store: &dyn CredentialStore,
    reference: &CredentialRef,
    secret: SecretString,
    persist: impl FnOnce() -> Result<T>,
) -> Result<T> {
    store.set(reference, secret)?;
    match persist() {
        Ok(value) => Ok(value),
        Err(error) => {
            let _ = store.delete(reference);
            Err(error)
        }
    }
}

fn entry_for(reference: &CredentialRef) -> Result<Entry> {
    let (service, account) = reference.system_store_parts()?;
    Entry::new(service, account).map_err(map_keyring_error)
}

fn nonempty_credential(value: String) -> Result<SecretString> {
    if value.is_empty() {
        Err(empty())
    } else {
        Ok(SecretString::from(value))
    }
}

fn map_keyring_error(error: KeyringError) -> SignalError {
    match error {
        KeyringError::NoEntry => missing(),
        _ => SignalError::Credential(STORE_FAILURE.to_owned()),
    }
}

fn redact_store_error(error: SignalError) -> SignalError {
    match error {
        SignalError::Credential(message) if message == MISSING_CREDENTIAL => missing(),
        SignalError::Credential(message) if message == STORE_FAILURE => {
            SignalError::Credential(STORE_FAILURE.to_owned())
        }
        _ => unavailable(),
    }
}

fn missing() -> SignalError {
    SignalError::Credential(MISSING_CREDENTIAL.to_owned())
}

fn empty() -> SignalError {
    SignalError::Credential(EMPTY_CREDENTIAL.to_owned())
}

fn unavailable() -> SignalError {
    SignalError::Credential(UNAVAILABLE_CREDENTIAL.to_owned())
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub struct MemoryCredentialStore {
    credentials: std::sync::Mutex<std::collections::HashMap<(String, String), SecretString>>,
}

#[cfg(any(test, feature = "test-support"))]
impl MemoryCredentialStore {
    pub fn expose_for_test(&self, reference: &CredentialRef) -> String {
        self.get(reference)
            .expect("test credential should be present")
            .expose_secret()
            .to_owned()
    }

    fn key(reference: &CredentialRef) -> Result<(String, String)> {
        let (service, account) = reference.system_store_parts()?;
        Ok((service.to_owned(), account.to_owned()))
    }
}

#[cfg(any(test, feature = "test-support"))]
impl CredentialStore for MemoryCredentialStore {
    fn set(&self, reference: &CredentialRef, secret: SecretString) -> Result<()> {
        self.credentials
            .lock()
            .expect("memory credential store mutex")
            .insert(Self::key(reference)?, secret);
        Ok(())
    }

    fn get(&self, reference: &CredentialRef) -> Result<SecretString> {
        self.credentials
            .lock()
            .expect("memory credential store mutex")
            .get(&Self::key(reference)?)
            .cloned()
            .ok_or_else(missing)
    }

    fn delete(&self, reference: &CredentialRef) -> Result<()> {
        self.credentials
            .lock()
            .expect("memory credential store mutex")
            .remove(&Self::key(reference)?);
        Ok(())
    }
}
