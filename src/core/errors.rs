use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::fmt;

/// Error type that represents different kinds of errors that can occur
/// in the dxLink protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DxLinkErrorType {
    /// An unknown error occurred
    Unknown,
    /// The protocol version is not supported
    UnsupportedProtocol,
    /// A timeout occurred while waiting for a response
    Timeout,
    /// Authentication failed
    Unauthorized,
    /// Received message could not be parsed
    InvalidMessage,
    /// The requested action is not valid
    BadAction,
}

/// Unified error type that encapsulates all possible errors
/// that can occur while using the dxLink client.
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub struct DxLinkError {
    /// Type of the error
    pub error_type: DxLinkErrorType,
    /// Detailed error message
    pub message: String,
}

impl fmt::Display for DxLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.error_type, self.message)
    }
}

/// Type alias for common Result type used throughout the library
pub type Result<T> = std::result::Result<T, DxLinkError>;

/// Callback type for error listeners
pub type ErrorListener = Box<dyn Fn(&DxLinkError) + Send + Sync>;

impl DxLinkError {
    /// Creates a new DxLinkError with the given type and message
    pub fn new(error_type: DxLinkErrorType, message: impl Into<String>) -> Self {
        Self {
            error_type,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_error_creation() {
        let error = DxLinkError::new(DxLinkErrorType::Timeout, "Connection timed out");
        assert_eq!(error.error_type, DxLinkErrorType::Timeout);
        assert_eq!(error.message, "Connection timed out");
    }

    #[test]
    fn test_error_display() {
        let error = DxLinkError::new(DxLinkErrorType::InvalidMessage, "Invalid JSON");
        assert_eq!(
            format!("{}", error),
            "InvalidMessage: Invalid JSON"
        );
    }

    #[test]
    fn test_error_clone() {
        let error = DxLinkError::new(DxLinkErrorType::Unknown, "Test error");
        let cloned = error.clone();
        assert_eq!(error.error_type, cloned.error_type);
        assert_eq!(error.message, cloned.message);
    }

    #[test]
    fn test_error_type_comparison() {
        assert_eq!(DxLinkErrorType::Timeout, DxLinkErrorType::Timeout);
        assert_ne!(DxLinkErrorType::Timeout, DxLinkErrorType::Unknown);
    }

    #[test]
    fn test_error_listener() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let error = DxLinkError::new(DxLinkErrorType::BadAction, "Invalid action");

        let called_clone = called.clone();
        let listener: ErrorListener = Box::new(move |e| {
            assert_eq!(e.error_type, DxLinkErrorType::BadAction);
            called_clone.store(true, Ordering::SeqCst);
        });

        listener(&error);
        assert!(called.load(Ordering::SeqCst));
    }
}
