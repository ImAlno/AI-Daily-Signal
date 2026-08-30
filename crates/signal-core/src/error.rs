#[derive(Debug, thiserror::Error)]
pub enum SignalError {
    #[error("operation cancelled")]
    Cancelled,
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("feed parse error: {0}")]
    Feed(String),
    #[error("refresh failed: {0}")]
    Refresh(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Credential(String),
    #[error(transparent)]
    Provider(#[from] crate::ProviderFailure),
}

pub type Result<T> = std::result::Result<T, SignalError>;
