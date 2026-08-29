use secrecy::{ExposeSecret, SecretString};
use signal_core::{CredentialRef, CredentialStore, SignalError, SystemCredentialStore};

struct CredentialCleanup {
    reference: CredentialRef,
}

impl Drop for CredentialCleanup {
    fn drop(&mut self) {
        let store = SystemCredentialStore;
        let _ = store.delete(&self.reference);
    }
}

#[test]
#[ignore = "requires an unlocked ephemeral OS credential store"]
fn system_credential_contract() -> signal_core::Result<()> {
    let reference = CredentialRef::for_profile(uuid::Uuid::new_v4());
    let _cleanup = CredentialCleanup {
        reference: reference.clone(),
    };
    let sentinel = format!("signal-credential-contract-{}", uuid::Uuid::new_v4());
    let store = SystemCredentialStore;

    store.set(&reference, SecretString::from(sentinel.clone()))?;
    let stored = store.get(&reference)?;
    assert!(
        stored.expose_secret() == sentinel,
        "credential round trip returned a different value"
    );

    store.delete(&reference)?;
    let missing = store.get(&reference);
    assert!(
        matches!(missing, Err(SignalError::Credential(message)) if message == "credential is missing"),
        "deleted credential was not reported missing"
    );

    Ok(())
}
