//! Feed service types and messages for DXLink
//!
//! This module contains the core types and message structures used by the feed service,
//! including contracts, data formats, subscriptions and event handling.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DxLinkError, DxLinkErrorType};

/// Contract types for feed service
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum FeedContract {
    /// Automatic contract selection
    Auto,
    /// Ticker data
    Ticker,
    /// Historical data
    History,
    /// Real-time stream
    Stream,
}

/// Data format for feed messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum FeedDataFormat {
    /// Full data format with named fields
    #[default]
    Full,
    /// Compact data format with arrays
    Compact,
}

/// Parameters for feed service channel request
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedParameters {
    /// Contract type for this feed
    pub contract: FeedContract,
}

/// Feed event fields configuration
pub type FeedEventFields = HashMap<String, Vec<String>>;

/// Setup message for feed service configuration
impl From<serde_json::Error> for DxLinkError {
    fn from(err: serde_json::Error) -> Self {
        DxLinkError {
            error_type: DxLinkErrorType::InvalidMessage,
            message: err.to_string(),
        }
    }
}

/// Message to configure feed service setup options
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedSetupMessage {
    /// Type of the message
    #[serde(rename = "type")]
    pub msg_type: String,
    /// Channel ID
    pub channel: u64,
    /// Acceptable aggregation period in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_aggregation_period: Option<u32>,
    /// Acceptable data format (full or compact)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_data_format: Option<FeedDataFormat>,
    /// Acceptable event field configurations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_event_fields: Option<FeedEventFields>,
}

/// Configuration message from feed service
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedConfigMessage {
    /// Type of the message
    #[serde(rename = "type")]
    pub msg_type: String,
    /// Channel ID
    pub channel: u64,
    /// Current aggregation period in milliseconds
    pub aggregation_period: u32,
    /// Current data format
    pub data_format: FeedDataFormat,
    /// Current event field configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_fields: Option<FeedEventFields>,
}

/// Basic subscription information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subscription {
    /// Type of subscription
    pub r#type: String,
    /// Symbol to subscribe to
    pub symbol: String,
}

/// Time series subscription with from_time
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeSeriesSubscription {
    /// Type of subscription
    pub r#type: String,
    /// Symbol to subscribe to
    pub symbol: String,
    /// Starting timestamp
    pub from_time: i64,
}

/// Indexed event subscription with source
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexedEventSubscription {
    /// Type of subscription
    pub r#type: String,
    /// Symbol to subscribe to
    pub symbol: String,
    /// Source of the indexed event
    pub source: String,
}

/// Subscription message for managing feed subscriptions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedSubscriptionMessage {
    /// Type of the message
    #[serde(rename = "type")]
    pub msg_type: String,
    /// Channel ID
    pub channel: u64,
    /// Subscriptions to add
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add: Option<Vec<Value>>,
    /// Subscriptions to remove
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove: Option<Vec<Value>>,
    /// Whether to reset all subscriptions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset: Option<bool>,
}

/// Feed event value types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FeedEventValue {
    /// Numeric value
    Number(f64),
    /// String value
    String(String),
    /// Boolean value
    Bool(bool),
}

/// Feed event data in full format
pub type FeedEventData = HashMap<String, FeedEventValue>;

/// Feed event data in compact format
pub type FeedCompactEventData = (String, Vec<FeedEventValue>);

/// Data message containing feed events
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedDataMessage {
    /// Type of the message
    #[serde(rename = "type")]
    pub msg_type: String,
    /// Channel ID
    pub channel: u64,
    /// Feed data payload
    #[serde(flatten)]
    pub data: FeedData,
}

/// Feed data variants
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FeedData {
    /// Full format feed data
    Full(Vec<FeedEventData>),
    /// Compact format feed data
    Compact(Vec<FeedCompactEventData>),
}

impl FeedData {
    /// Check if data is in full format
    pub fn is_full(&self) -> bool {
        matches!(self, FeedData::Full(_))
    }

    /// Check if data is in compact format
    pub fn is_compact(&self) -> bool {
        matches!(self, FeedData::Compact(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_feed_contract_serialization() {
        assert_eq!(
            serde_json::to_string(&FeedContract::Auto).unwrap(),
            "\"AUTO\""
        );
        assert_eq!(
            serde_json::from_str::<FeedContract>("\"AUTO\"").unwrap(),
            FeedContract::Auto
        );
    }

    #[test]
    fn test_feed_data_format_serialization() {
        assert_eq!(
            serde_json::to_string(&FeedDataFormat::Compact).unwrap(),
            "\"COMPACT\""
        );
        assert_eq!(
            serde_json::from_str::<FeedDataFormat>("\"COMPACT\"").unwrap(),
            FeedDataFormat::Compact
        );
    }

    #[test]
    fn test_feed_subscription_message() {
        let msg = FeedSubscriptionMessage {
            msg_type: "FEED_SUBSCRIPTION".to_string(),
            channel: 1,
            add: Some(vec![json!({
                "type": "Quote",
                "symbol": "AAPL"
            })]),
            remove: None,
            reset: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        let decoded: FeedSubscriptionMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }
}
