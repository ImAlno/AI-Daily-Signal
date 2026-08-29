mod error;

pub use error::{Result, SignalError};

#[cfg(test)]
mod tests {
    #[test]
    fn signal_error_is_public() {
        let error = crate::SignalError::InvalidConfiguration("bad source".into());
        assert_eq!(error.to_string(), "invalid configuration: bad source");
    }
}
