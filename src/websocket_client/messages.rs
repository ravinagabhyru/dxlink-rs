use async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::core::errors::{DxLinkError, DxLinkErrorType};

use crate::dom::messages::{DomConfigMessage, DomSetupMessage, DomSnapshotMessage};
use crate::feed::messages::{
    FeedConfigMessage, FeedDataMessage, FeedSetupMessage, FeedSubscriptionMessage,
};

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
    // FEED service messages
    FeedSetup(FeedSetupMessage),
    FeedConfig(FeedConfigMessage),
    FeedSubscription(FeedSubscriptionMessage),
    FeedData(FeedDataMessage),
    // DOM service messages
    DomSetup(DomSetupMessage),
    DomConfig(DomConfigMessage),
    DomSnapshot(DomSnapshotMessage),
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
            // FEED messages
            MessageType::FeedSetup(_) => "FEED_SETUP",
            MessageType::FeedConfig(_) => "FEED_CONFIG",
            MessageType::FeedSubscription(_) => "FEED_SUBSCRIPTION",
            MessageType::FeedData(_) => "FEED_DATA",
            // DOM messages
            MessageType::DomSetup(_) => "DOM_SETUP",
            MessageType::DomConfig(_) => "DOM_CONFIG",
            MessageType::DomSnapshot(_) => "DOM_SNAPSHOT",
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
            // FEED messages
            MessageType::FeedSetup(m) => m.channel,
            MessageType::FeedConfig(m) => m.channel,
            MessageType::FeedSubscription(m) => m.channel,
            MessageType::FeedData(m) => m.channel,
            // DOM messages
            MessageType::DomSetup(m) => m.channel,
            MessageType::DomConfig(m) => m.channel,
            MessageType::DomSnapshot(m) => m.channel,
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
            // FEED messages
            MessageType::FeedSetup(m) => serde_json::to_value(m).unwrap(),
            MessageType::FeedConfig(m) => serde_json::to_value(m).unwrap(),
            MessageType::FeedSubscription(m) => serde_json::to_value(m).unwrap(),
            MessageType::FeedData(m) => serde_json::to_value(m).unwrap(),
            // DOM messages
            MessageType::DomSetup(m) => serde_json::to_value(m).unwrap(),
            MessageType::DomConfig(m) => serde_json::to_value(m).unwrap(),
            MessageType::DomSnapshot(m) => serde_json::to_value(m).unwrap(),
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
    fn message_type(&self) -> &'static str {
        "AUTH_STATE"
    }

    fn channel(&self) -> u64 {
        self.channel
    }

    fn payload(&self) -> Value {
        json!({
            "state": self.state
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl TryFrom<serde_json::Value> for AuthStateMessage {
    type Error = Box<dyn std::error::Error>;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        let message_type = value["messageType"]
            .as_str()
            .ok_or_else(|| DxLinkError::new(DxLinkErrorType::BadAction, "Missing messageType"))?
            .to_string();

        let channel = value["channel"]
            .as_u64()
            .ok_or_else(|| DxLinkError::new(DxLinkErrorType::BadAction, "Missing channel"))?;

        let state = value["payload"]["state"]
            .as_str()
            .ok_or_else(|| DxLinkError::new(DxLinkErrorType::BadAction, "Missing state"))?
            .to_string();

        Ok(AuthStateMessage {
            message_type,
            channel,
            state,
        })
    }
}

/// Message for connection setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    pub version: String,
    #[serde(rename = "keepaliveTimeout", skip_serializing_if = "Option::is_none")]
    pub keepalive_timeout: Option<u64>,
    #[serde(
        rename = "acceptKeepaliveTimeout",
        skip_serializing_if = "Option::is_none"
    )]
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
        msg_type == "CHANNEL_OPENED" || msg_type == "CHANNEL_CLOSED" || msg_type == "ERROR"
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
        assert!(json.contains("\"keepaliveTimeout\":60"));
        assert!(json.contains("\"acceptKeepaliveTimeout\":60"));
    }

    // Task 5: Serialization Tests (Client -> Server)
    #[test]
    fn test_auth_message_serialization() {
        let msg = AuthMessage {
            message_type: "AUTH".to_string(),
            channel: 0,
            token: "token#123".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"AUTH\""));
        assert!(json.contains("\"channel\":0"));
        assert!(json.contains("\"token\":\"token#123\""));
    }

    #[test]
    fn test_channel_request_message_serialization() {
        let msg = ChannelRequestMessage {
            message_type: "CHANNEL_REQUEST".to_string(),
            channel: 1,
            service: "FEED".to_string(),
            parameters: Some(serde_json::json!({
                "contract": "AUTO"
            })),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"CHANNEL_REQUEST\""));
        assert!(json.contains("\"channel\":1"));
        assert!(json.contains("\"service\":\"FEED\""));
        assert!(json.contains("\"parameters\":{\"contract\":\"AUTO\"}"));
    }

    #[test]
    fn test_channel_cancel_message_serialization() {
        let msg = ChannelCancelMessage {
            message_type: "CHANNEL_CANCEL".to_string(),
            channel: 1,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"CHANNEL_CANCEL\""));
        assert!(json.contains("\"channel\":1"));
    }

    // Moved to feed::messages module
    // #[test]
    // fn test_feed_setup_message_serialization() {
    //     /* Test moved to feed::messages module */
    // }

    // Moved to feed::messages module
    // #[test]
    // fn test_feed_subscription_message_serialization() {
    //     /* Test moved to feed::messages module */
    // }

    // Moved to dom::messages module
    // #[test]
    // fn test_dom_setup_message_serialization() {
    //     /* Test moved to dom::messages module */
    // }

    // Task 6: Deserialization Tests (Server -> Client - Part 1)
    #[test]
    fn test_setup_message_deserialization() {
        let json = r#"{
            "type": "SETUP",
            "channel": 0,
            "version": "1.0.0",
            "keepaliveTimeout": 60,
            "acceptKeepaliveTimeout": 60
        }"#;

        let msg: SetupMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "SETUP");
        assert_eq!(msg.channel, 0);
        assert_eq!(msg.version, "1.0.0");
        assert_eq!(msg.keepalive_timeout, Some(60));
        assert_eq!(msg.accept_keepalive_timeout, Some(60));
    }

    #[test]
    fn test_auth_state_message_deserialization() {
        let json = r#"{
            "type": "AUTH_STATE",
            "channel": 0,
            "state": "AUTHORIZED"
        }"#;

        let msg: AuthStateMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "AUTH_STATE");
        assert_eq!(msg.channel, 0);
        assert_eq!(msg.state, "AUTHORIZED");
    }

    #[test]
    fn test_keepalive_message_deserialization() {
        let json = r#"{
            "type": "KEEPALIVE",
            "channel": 0
        }"#;

        let msg: KeepaliveMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "KEEPALIVE");
        assert_eq!(msg.channel, 0);
    }

    #[test]
    fn test_channel_opened_message_deserialization() {
        let json = r#"{
            "type": "CHANNEL_OPENED",
            "channel": 1,
            "service": "FEED",
            "parameters": {
                "contract": "AUTO"
            }
        }"#;

        let msg: ChannelOpenedMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "CHANNEL_OPENED");
        assert_eq!(msg.channel, 1);
        assert_eq!(msg.service, "FEED");
        assert!(msg.parameters.is_some());
        assert_eq!(
            msg.parameters
                .as_ref()
                .unwrap()
                .get("contract")
                .unwrap()
                .as_str()
                .unwrap(),
            "AUTO"
        );
    }

    #[test]
    fn test_channel_closed_message_deserialization() {
        let json = r#"{
            "type": "CHANNEL_CLOSED",
            "channel": 1
        }"#;

        let msg: ChannelClosedMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "CHANNEL_CLOSED");
        assert_eq!(msg.channel, 1);
    }

    #[test]
    fn test_error_message_deserialization() {
        let json = r#"{
            "type": "ERROR",
            "channel": 0,
            "error": "timeout",
            "message": "Connection timed out"
        }"#;

        let msg: ErrorMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "ERROR");
        assert_eq!(msg.channel, 0);
        assert_eq!(msg.error, DxLinkErrorType::Timeout);
        assert_eq!(msg.message, "Connection timed out");
    }

    // Task 7: Deserialization Tests (Server -> Client - Part 2: FEED)
    // Moved to feed::messages module
    // #[test]
    // fn test_feed_config_message_deserialization() {
    //     /* Test moved to feed::messages module */
    // }

    // Moved to feed::messages module
    // #[test]
    // fn test_feed_data_full_message_deserialization() {
    //     /* Test moved to feed::messages module */
    // }

    // Moved to feed::messages module
    // #[test]
    // fn test_feed_data_compact_message_deserialization() {
    //     /* Test moved to feed::messages module */
    // }

    // Task 8: Deserialization Tests (Server -> Client - Part 3: DOM)
    // Moved to dom::messages module
    // #[test]
    // fn test_dom_config_message_deserialization() {
    //     /* Test moved to dom::messages module */
    // }

    // Moved to dom::messages module
    // #[test]
    // fn test_dom_snapshot_message_deserialization() {
    //     /* Test moved to dom::messages module */
    // }
}
