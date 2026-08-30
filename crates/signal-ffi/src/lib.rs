mod client;
mod error;
mod types;

pub use client::CompanionClient;
pub use error::CompanionError;
pub use types::*;

uniffi::setup_scaffolding!();
