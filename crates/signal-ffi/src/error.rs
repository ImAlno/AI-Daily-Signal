use signal_core::{ProviderFailureKind, SignalError};

#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
pub enum CompanionError {
    #[error("setup is incomplete")]
    NotInitialized,
    #[error("input is invalid")]
    InvalidInput,
    #[error("item was not found")]
    NotFound,
    #[error("credential is unavailable")]
    CredentialUnavailable,
    #[error("provider consent is required")]
    ConsentRequired,
    #[error("daily budget is exhausted")]
    BudgetExhausted,
    #[error("provider is unavailable")]
    ProviderUnavailable,
    #[error("network is unavailable")]
    Offline,
    #[error("refresh is already running")]
    RefreshAlreadyRunning,
    #[error("operation was cancelled")]
    Cancelled,
    #[error("local storage is unavailable")]
    StorageUnavailable,
}

impl From<SignalError> for CompanionError {
    fn from(error: SignalError) -> Self {
        match error {
            SignalError::Cancelled => Self::Cancelled,
            SignalError::InvalidConfiguration(_) => Self::InvalidInput,
            SignalError::Io(_)
            | SignalError::Database(_)
            | SignalError::Serialization(_)
            | SignalError::Storage(_) => Self::StorageUnavailable,
            SignalError::Network(_) | SignalError::Feed(_) | SignalError::Refresh(_) => {
                Self::Offline
            }
            SignalError::NotFound(_) => Self::NotFound,
            SignalError::Credential(_) => Self::CredentialUnavailable,
            SignalError::Provider(error) => match error.kind() {
                ProviderFailureKind::Cancelled => Self::Cancelled,
                ProviderFailureKind::CredentialMissing | ProviderFailureKind::Authentication => {
                    Self::CredentialUnavailable
                }
                ProviderFailureKind::Timeout | ProviderFailureKind::Transport => Self::Offline,
                ProviderFailureKind::RateLimited
                | ProviderFailureKind::ProviderRejected
                | ProviderFailureKind::ProviderUnavailable
                | ProviderFailureKind::MalformedOutput => Self::ProviderUnavailable,
            },
        }
    }
}
