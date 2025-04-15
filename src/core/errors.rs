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

/// Channel-specific error types
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: super::channel::DxLinkChannelState,
        to: super::channel::DxLinkChannelState,
    },

    #[error("Channel not ready. Current state: {state:?}")]
    NotReady {
        state: super::channel::DxLinkChannelState,
    },

    #[error("Unsupported message type: {message_type}. Supported types: {supported}")]
    UnsupportedMessageType {
        message_type: String,
        supported: String,
    },

    #[error("Invalid message payload: {0}")]
    InvalidPayload(String),

    #[error("Send error: {0}")]
    SendError(String),

    #[error("Operation timeout after {timeout_secs} seconds")]
    Timeout {
        timeout_secs: u64,
    },

    #[error("Channel closed")]
    Closed,

    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl From<ChannelError> for DxLinkError {
    fn from(err: ChannelError) -> Self {
        match err {
            ChannelError::InvalidStateTransition { .. } => DxLinkError {
                error_type: DxLinkErrorType::BadAction,
                message: err.to_string(),
            },
            ChannelError::NotReady { .. } => DxLinkError {
                error_type: DxLinkErrorType::BadAction,
                message: err.to_string(),
            },
            ChannelError::UnsupportedMessageType { .. } => DxLinkError {
                error_type: DxLinkErrorType::InvalidMessage,
                message: err.to_string(),
            },
            ChannelError::InvalidPayload(_) => DxLinkError {
                error_type: DxLinkErrorType::InvalidMessage,
                message: err.to_string(),
            },
            ChannelError::SendError(_) => DxLinkError {
                error_type: DxLinkErrorType::Unknown,
                message: err.to_string(),
            },
            ChannelError::Timeout { .. } => DxLinkError {
                error_type: DxLinkErrorType::Timeout,
                message: err.to_string(),
            },
            ChannelError::Closed => DxLinkError {
                error_type: DxLinkErrorType::BadAction,
                message: err.to_string(),
            },
            ChannelError::Other(e) => DxLinkError {
                error_type: DxLinkErrorType::Unknown,
                message: e.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use serde_json::json;

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

    #[test]
    fn test_error_type_deserialization() {
        // Test all possible error types from JSON
        let json_values = [
            (json!("timeout"), DxLinkErrorType::Timeout),
            (json!("unsupported_protocol"), DxLinkErrorType::UnsupportedProtocol),
            (json!("unauthorized"), DxLinkErrorType::Unauthorized),
            (json!("invalid_message"), DxLinkErrorType::InvalidMessage),
            (json!("bad_action"), DxLinkErrorType::BadAction),
            (json!("unknown"), DxLinkErrorType::Unknown),
        ];

        for (json_value, expected_type) in json_values {
            let error_type: DxLinkErrorType = serde_json::from_value(json_value).unwrap();
            assert_eq!(error_type, expected_type);
        }
    }

    #[test]
    fn test_error_message_parsing() {
        use crate::websocket_client::messages::ErrorMessage;

        // Test parsing an ERROR message with every possible error type
        let error_types = [
            ("timeout", DxLinkErrorType::Timeout),
            ("unsupported_protocol", DxLinkErrorType::UnsupportedProtocol),
            ("unauthorized", DxLinkErrorType::Unauthorized),
            ("invalid_message", DxLinkErrorType::InvalidMessage),
            ("bad_action", DxLinkErrorType::BadAction),
            ("unknown", DxLinkErrorType::Unknown),
        ];

        for (error_str, expected_type) in error_types {
            let json = format!(r#"{{
                "type": "ERROR",
                "channel": 0,
                "error": "{}",
                "message": "Error description"
            }}"#, error_str);

            let msg: ErrorMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(msg.message_type, "ERROR");
            assert_eq!(msg.channel, 0);
            assert_eq!(msg.error, expected_type);
            assert_eq!(msg.message, "Error description");
        }
    }
    
    #[test]
    fn test_detailed_error_message_parsing() {
        use crate::websocket_client::messages::ErrorMessage;
        
        // Test parsing ERROR messages with different channel IDs and error types
        let test_cases = [
            // Connection-level errors (channel 0)
            (0, "timeout", "Connection timed out", DxLinkErrorType::Timeout),
            (0, "unsupported_protocol", "Protocol version not supported", DxLinkErrorType::UnsupportedProtocol),
            (0, "unauthorized", "Authentication failed", DxLinkErrorType::Unauthorized),
            
            // Channel-specific errors
            (1, "invalid_message", "Invalid subscription format", DxLinkErrorType::InvalidMessage),
            (2, "bad_action", "Cannot subscribe while channel is closing", DxLinkErrorType::BadAction),
            (5, "timeout", "Operation timed out", DxLinkErrorType::Timeout),
        ];
        
        for (channel, error_type, message, expected_type) in test_cases {
            let json = format!(r#"{{
                "type": "ERROR",
                "channel": {},
                "error": "{}",
                "message": "{}"
            }}"#, channel, error_type, message);
            
            let msg: ErrorMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(msg.message_type, "ERROR");
            assert_eq!(msg.channel, channel);
            assert_eq!(msg.error, expected_type);
            assert_eq!(msg.message, message);
            
            // Verify the error can be converted to a DxLinkError
            let error = DxLinkError::new(msg.error, msg.message.clone());
            assert_eq!(error.error_type, expected_type);
            assert_eq!(error.message, message);
        }
    }

    #[test]
    fn test_channel_error_conversion() {
        use crate::core::channel::DxLinkChannelState;

        // Test converting ChannelError to DxLinkError
        let channel_errors = [
            (
                ChannelError::InvalidStateTransition {
                    from: DxLinkChannelState::Requested,
                    to: DxLinkChannelState::Closed,
                },
                DxLinkErrorType::BadAction,
            ),
            (
                ChannelError::NotReady {
                    state: DxLinkChannelState::Requested,
                },
                DxLinkErrorType::BadAction,
            ),
            (
                ChannelError::UnsupportedMessageType {
                    message_type: "UNKNOWN".to_string(),
                    supported: "FEED_SETUP, FEED_SUBSCRIPTION".to_string(),
                },
                DxLinkErrorType::InvalidMessage,
            ),
            (
                ChannelError::InvalidPayload("Invalid JSON".to_string()),
                DxLinkErrorType::InvalidMessage,
            ),
            (
                ChannelError::SendError("Failed to send message".to_string()),
                DxLinkErrorType::Unknown,
            ),
            (
                ChannelError::Timeout { timeout_secs: 30 },
                DxLinkErrorType::Timeout,
            ),
            (
                ChannelError::Closed,
                DxLinkErrorType::BadAction,
            ),
        ];

        for (channel_error, expected_type) in channel_errors {
            let dxlink_error: DxLinkError = channel_error.into();
            assert_eq!(dxlink_error.error_type, expected_type);
        }
    }
}
