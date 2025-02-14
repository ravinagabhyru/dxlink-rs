use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use async_trait;

use crate::DxLinkErrorType;

/// Authentication state for the connection
// #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
// #[serde(rename_all = "UPPERCASE")]
// pub enum AuthState {
//     /// Client is authorized
//     Authorized,
//     /// Client is unauthorized
//     Unauthorized,
// }
//

// use serde::de::{self};

// Add this before your MessageType enum
// impl<'de> Deserialize<'de> for MessageType {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: serde::Deserializer<'de>,
//     {
//         // First, deserialize to Value to inspect the raw data
//         let value = Value::deserialize(deserializer)?;

//         debug!("Attempting to deserialize message: {}", value);

//         // Try to extract the message type
//         let type_str = value.get("type")
//             .and_then(Value::as_str)
//             .ok_or_else(|| de::Error::missing_field("type"))?;

//         // Match on message type and deserialize to specific variant
//         let result = match type_str {
//             "AUTH" => serde_json::from_value(value.clone())
//                 .map(MessageType::Auth)
//                 .map_err(|e| de::Error::custom(format!("Auth deserialize error: {}", e))),
//             "AUTH_STATE" => serde_json::from_value(value.clone())
//                 .map(MessageType::AuthState)
//                 .map_err(|e| de::Error::custom(format!("AuthState deserialize error: {}", e))),
//             "SETUP" => serde_json::from_value(value.clone())
//                 .map(MessageType::Setup)
//                 .map_err(|e| de::Error::custom(format!("Setup deserialize error: {}", e))),
//             // ... add other variants
//             _ => Err(de::Error::custom(format!("Unknown message type: {}", type_str))),
//         };

//         debug!("Deserialization result: {:?}", result);
//         result
//     }
// }

/// Enum representing all possible message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageType {
    Auth(AuthMessage),
    AuthState(AuthStateMessage),
    Setup(SetupMessage),
    KeepAlive(KeepaliveMessage),
    ChannelRequest(ChannelRequestMessage),
    ChannelCancel(ChannelCancelMessage),
    ChannelOpened(ChannelOpenedMessage),
    ChannelClosed(ChannelClosedMessage),
    Error(ErrorMessage),
}

impl Message for MessageType {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn message_type(&self) -> &'static str {
        match self {
            MessageType::Auth(_) => "AUTH",
            MessageType::AuthState(_) => "AUTH_STATE",
            MessageType::Setup(_) => "SETUP",
            MessageType::KeepAlive(_) => "KEEPALIVE",
            MessageType::ChannelRequest(_) => "CHANNEL_REQUEST",
            MessageType::ChannelCancel(_) => "CHANNEL_CANCEL",
            MessageType::ChannelOpened(_) => "CHANNEL_OPENED",
            MessageType::ChannelClosed(_) => "CHANNEL_CLOSED",
            MessageType::Error(_) => "ERROR",
        }
    }

    fn channel(&self) -> u64 {
        match self {
            MessageType::Auth(m) => m.channel,
            MessageType::AuthState(m) => m.channel,
            MessageType::Setup(m) => m.channel,
            MessageType::KeepAlive(m) => m.channel,
            MessageType::ChannelRequest(m) => m.channel,
            MessageType::ChannelCancel(m) => m.channel,
            MessageType::ChannelOpened(m) => m.channel,
            MessageType::ChannelClosed(m) => m.channel,
            MessageType::Error(m) => m.channel,
        }
    }

    fn payload(&self) -> Value {
        match self {

            MessageType::Auth(m) => serde_json::to_value(m).unwrap(),
            MessageType::AuthState(m) => serde_json::to_value(m).unwrap(),
            MessageType::Setup(m) => serde_json::to_value(m).unwrap(),
            MessageType::KeepAlive(m) => serde_json::to_value(m).unwrap(),
            MessageType::ChannelRequest(m) => serde_json::to_value(m).unwrap(),
            MessageType::ChannelCancel(m) => serde_json::to_value(m).unwrap(),
            MessageType::ChannelOpened(m) => serde_json::to_value(m).unwrap(),
            MessageType::ChannelClosed(m) => serde_json::to_value(m).unwrap(),
            MessageType::Error(m) => serde_json::to_value(m).unwrap(),
        }
    }
}

/// Base trait for all messages
#[async_trait::async_trait]
pub trait Message: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
    fn message_type(&self) -> &'static str;
    fn channel(&self) -> u64;
    fn payload(&self) -> Value;
}

/// Message for client authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    pub token: String,
}

impl Message for AuthMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "AUTH"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}
/// Message for authentication state changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStateMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    pub state: String,
}

impl Message for AuthStateMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "AUTH_STATE"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message for connection setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keepalive_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_keepalive_timeout: Option<u64>,
}

impl Message for SetupMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "SETUP"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message for maintaining connection alive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepaliveMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
}

impl Message for KeepaliveMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "KEEPALIVE"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message for channel requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelRequestMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

impl Message for ChannelRequestMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "CHANNEL_REQUEST"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message for canceling channel requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCancelMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
}

impl Message for ChannelCancelMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "CHANNEL_CANCEL"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message indicating a channel has been opened
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelOpenedMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<HashMap<String, Value>>,
}

impl Message for ChannelOpenedMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "CHANNEL_OPENED"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message indicating a channel has been closed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelClosedMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
}

impl Message for ChannelClosedMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "CHANNEL_CLOSED"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Error message for connection-level errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    pub error: DxLinkErrorType,
    pub message: String,
}

impl Message for ErrorMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "ERROR"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message utilities
pub mod util {
    use super::*;

    /// Check if a message is a connection message (channel 0)
    // pub fn is_connection_message<T: Message>(message: &T) -> bool {
    pub fn is_connection_message(message: &Box<dyn Message + Send + Sync>) -> bool {
        let msg_type = message.message_type();
        msg_type == "SETUP"
            || msg_type == "AUTH"
            || msg_type == "AUTH_STATE"
            || msg_type == "KEEPALIVE"
            || msg_type == "ERROR"
    }

    /// Check if a message is a channel message (channel != 0)
    pub fn is_channel_message(message: &Box<dyn Message + Send + Sync>) -> bool {
        message.channel() != 0
    }

    /// Check if a message is a channel lifecycle message
    pub fn is_channel_lifecycle_message(message: &Box<dyn Message + Send + Sync>) -> bool {
        let msg_type = message.message_type();
        msg_type == "CHANNEL_OPENED"
            || msg_type == "CHANNEL_CLOSED"
            || msg_type == "ERROR"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_type_detection() {
        let setup_msg = SetupMessage {
            message_type: "SETUP".to_string(),
            channel: 0,
            version: "1.0".to_string(),
            keepalive_timeout: None,
            accept_keepalive_timeout: None,
        };

        let boxed_setup_msg: Box<dyn Message + Send + Sync> = Box::new(setup_msg);
        assert!(util::is_connection_message(&boxed_setup_msg));
        assert!(!util::is_channel_message(&boxed_setup_msg));

        let channel_msg = ChannelRequestMessage {
            message_type: "CHANNEL_REQUEST".to_string(),
            channel: 1,
            service: "test".to_string(),
            parameters: None,
        };

        let boxed_channel_msg: Box<dyn Message + Send + Sync> = Box::new(channel_msg);
        assert!(!util::is_connection_message(&boxed_channel_msg));
        assert!(util::is_channel_message(&boxed_channel_msg));
    }

    #[test]
    fn test_serialization() {
        let setup_msg = SetupMessage {
            message_type: "SETUP".to_string(),
            channel: 0,
            version: "1.0".to_string(),
            keepalive_timeout: Some(60),
            accept_keepalive_timeout: Some(60),
        };

        let json = serde_json::to_string(&setup_msg).unwrap();
        assert!(json.contains("\"type\":\"SETUP\""));
        assert!(json.contains("\"channel\":0"));
        assert!(json.contains("\"version\":\"1.0\""));
        assert!(json.contains("\"keepalive_timeout\":60"));
        assert!(json.contains("\"accept_keepalive_timeout\":60"));
    }
}
