//! Feed service types and messages for DXLink
//!
//! This module contains the core types and message structures used by the feed service,
//! including contracts, data formats, subscriptions and event handling.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DxLinkError, DxLinkErrorType};
use crate::websocket_client::messages::Message;

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

/// Feed subscription entry representing a symbol to subscribe to
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedSubscriptionEntry {
    /// Event type (e.g., "Quote", "Trade", "Candle")
    pub r#type: String,
    /// Symbol to subscribe to (e.g., "AAPL")
    pub symbol: String,
    /// Source of the feed (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Timestamp from when we want the data (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_time: Option<u64>,
}

/// Message for configuring the FEED service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedSetupMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_aggregation_period: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_data_format: Option<FeedDataFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_event_fields: Option<FeedEventFields>,
}

impl Message for FeedSetupMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "FEED_SETUP"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message with FEED service configuration from server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedConfigMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    #[serde(rename = "aggregationPeriod")]
    pub aggregation_period: u32,
    #[serde(rename = "dataFormat")]
    pub data_format: FeedDataFormat,
    #[serde(rename = "eventFields", skip_serializing_if = "Option::is_none")]
    pub event_fields: Option<FeedEventFields>,
}

impl Message for FeedConfigMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "FEED_CONFIG"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message for managing FEED subscriptions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedSubscriptionMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add: Option<Vec<FeedSubscriptionEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove: Option<Vec<FeedSubscriptionEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset: Option<bool>,
}

impl Message for FeedSubscriptionMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "FEED_SUBSCRIPTION"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Feed data in either full or compact format
#[derive(Debug, Clone)]
pub enum FeedData {
    /// Full format data (array of JSON objects)
    Full(Vec<Value>),
    /// Compact format data (array starting with a string event type followed by array of values)
    Compact(Vec<Value>),
}

impl Serialize for FeedData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            FeedData::Full(events) => events.serialize(serializer),
            FeedData::Compact(events) => events.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for FeedData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data: Vec<Value> = Vec::deserialize(deserializer)?;
        
        // If empty, treat as full format
        if data.is_empty() {
            return Ok(FeedData::Full(data));
        }
        
        // If first element is a string, and second element is an array, it's compact format
        if data.len() >= 2 && data[0].is_string() && data[1].is_array() {
            Ok(FeedData::Compact(data))
        } else {
            // Otherwise it's full format (array of objects)
            Ok(FeedData::Full(data))
        }
    }
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

/// Message containing FEED service data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedDataMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    pub data: FeedData,
}

impl Message for FeedDataMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "FEED_DATA"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

// Compatibility for existing feed module code
// These types can be removed if the feed module is refactored to use the new types

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

#[cfg(test)]
mod tests {
    use super::*;

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
            message_type: "FEED_SUBSCRIPTION".to_string(),
            channel: 1,
            add: Some(vec![
                FeedSubscriptionEntry {
                    r#type: "Quote".to_string(),
                    symbol: "AAPL".to_string(),
                    source: None,
                    from_time: None,
                },
            ]),
            remove: None,
            reset: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FEED_SUBSCRIPTION\""));
        assert!(json.contains("\"channel\":1"));
        assert!(json.contains("\"add\":[{"));
        assert!(json.contains("\"type\":\"Quote\""));
        assert!(json.contains("\"symbol\":\"AAPL\""));
    }
    
    #[test]
    fn test_feed_data_full_message_deserialization() {
        let json = r#"{
            "type": "FEED_DATA",
            "channel": 1,
            "data": [
                {
                    "eventSymbol": "AAPL",
                    "eventType": "Quote",
                    "bidPrice": 123.45,
                    "askPrice": 123.46,
                    "bidSize": 100,
                    "askSize": 200
                }
            ]
        }"#;

        let msg: FeedDataMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "FEED_DATA");
        assert_eq!(msg.channel, 1);
        
        // Check if data is in full format (not compact)
        assert!(msg.data.is_full());
        
        if let FeedData::Full(events) = &msg.data {
            assert_eq!(events.len(), 1);
            let event = &events[0];
            assert_eq!(event["eventSymbol"], "AAPL");
            assert_eq!(event["eventType"], "Quote");
            assert_eq!(event["bidPrice"], 123.45);
        } else {
            panic!("Expected Full data format");
        }
    }

    #[test]
    fn test_feed_data_compact_message_deserialization() {
        let json = r#"{
            "type": "FEED_DATA",
            "channel": 1,
            "data": [
                "Quote",
                ["AAPL", "Quote", 123.45, 123.46, 100, 200]
            ]
        }"#;

        let msg: FeedDataMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "FEED_DATA");
        assert_eq!(msg.channel, 1);
        
        assert!(msg.data.is_compact());
        
        if let FeedData::Compact(data) = &msg.data {
            assert_eq!(data.len(), 2);
            assert_eq!(data[0], "Quote");
            assert!(data[1].is_array());
            let values = data[1].as_array().unwrap();
            assert_eq!(values.len(), 6);
            assert_eq!(values[0], "AAPL");
        } else {
            panic!("Expected Compact data format");
        }
    }
}
