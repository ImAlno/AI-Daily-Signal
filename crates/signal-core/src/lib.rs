mod config;
mod domain;
mod error;
mod paths;

pub use config::{AppConfig, BriefingConfig, ConfigRepository};
pub use domain::{Briefing, BriefingItem, Candidate, ScoreBreakdown, Source, SourceKind, Story};
pub use error::{Result, SignalError};
pub use paths::AppPaths;

#[cfg(test)]
mod tests {
    #[test]
    fn signal_error_is_public() {
        let error = crate::SignalError::InvalidConfiguration("bad source".into());
        assert_eq!(error.to_string(), "invalid configuration: bad source");
    }
}
