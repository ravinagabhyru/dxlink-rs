//! Feed service types and messages for DXLink
//!
//! This module contains the core types and message structures used by the feed service,
//! including contracts, data formats, subscriptions and event handling.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DxLinkError, DxLinkErrorType};
use crate::websocket_client::messages::Message;
use crate::feed::events::FeedEvent;

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

/// Setup message for feed service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedSetupMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_aggregation_period: Option<f64>,
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
    pub aggregation_period: f64,
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
#[derive(Debug, Clone, PartialEq)]
pub enum FeedData {
    /// Full format data (array of JSON objects with named fields)
    Full(Vec<FeedEvent>),
    /// Compact format data (array of pairs: event type string followed by array of values)
    Compact(Vec<(String, Vec<Value>)>),
}

impl Serialize for FeedData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            FeedData::Full(events) => {
                // Serialize as an array of FeedEvent objects
                events.serialize(serializer)
            },
            FeedData::Compact(events) => {
                // Serialize as a flat array with alternating string type and value array
                let mut flat_array: Vec<Value> = Vec::new();
                for (event_type, values) in events {
                    flat_array.push(Value::String(event_type.clone()));
                    flat_array.push(Value::Array(values.clone()));
                }
                flat_array.serialize(serializer)
            },
        }
    }
}

impl<'de> Deserialize<'de> for FeedData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // First deserialize as a generic Value to determine format
        let data: Value = Value::deserialize(deserializer)?;

        if !data.is_array() {
            return Err(serde::de::Error::custom("Expected array for FeedData"));
        }

        let array = data.as_array().unwrap();

        // If empty, treat as full format (empty events list)
        if array.is_empty() {
            return Ok(FeedData::Full(Vec::new()));
        }

        // Check for compact format: first item is a string, second is an array
        if array.len() >= 2 && array[0].is_string() && array[1].is_array() {
            // This is compact format
            let mut compact_events = Vec::new();

            // Process pairs of items (event_type string followed by values array)
            let mut i = 0;
            while i < array.len() - 1 {
                if array[i].is_string() && array[i+1].is_array() {
                    let event_type = array[i].as_str().unwrap().to_string();
                    let values = array[i+1].as_array().unwrap().clone();
                    compact_events.push((event_type, values));
                    i += 2;
                } else {
                    // Skip invalid entries
                    i += 1;
                }
            }

            Ok(FeedData::Compact(compact_events))
        } else {
            // Try to deserialize as Full format (array of FeedEvent objects)
            match serde_json::from_value::<Vec<FeedEvent>>(data.clone()) {
                Ok(events) => Ok(FeedData::Full(events)),
                Err(err) => Err(serde::de::Error::custom(format!(
                    "Could not deserialize as either FULL or COMPACT format: {}", err
                ))),
            }
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

    /// Convert compact format to full format using event fields configuration
    pub fn compact_to_full(&self, event_fields: &FeedEventFields) -> Result<Vec<FeedEvent>, DxLinkError> {
        match self {
            FeedData::Full(events) => Ok(events.clone()),
            FeedData::Compact(compact_data) => {
                let mut full_events = Vec::new();

                for (event_type, values) in compact_data {
                    // Get field names for this event type
                    let field_names = match event_fields.get(event_type) {
                        Some(fields) => fields,
                        None => return Err(DxLinkError {
                            error_type: DxLinkErrorType::InvalidMessage,
                            message: format!("No field configuration for event type: {}", event_type),
                        }),
                    };

                    if values.len() > field_names.len() {
                        return Err(DxLinkError {
                            error_type: DxLinkErrorType::InvalidMessage,
                            message: format!(
                                "Too many values for event type {}: expected {}, got {}",
                                event_type, field_names.len(), values.len()
                            ),
                        });
                    }

                    // Construct JSON object with field names and values
                    let mut obj = serde_json::Map::new();
                    obj.insert("eventType".to_string(), Value::String(event_type.clone()));

                    for (i, value) in values.iter().enumerate() {
                        if i < field_names.len() {
                            obj.insert(field_names[i].clone(), value.clone());
                        }
                    }

                    // Deserialize as FeedEvent
                    let event_value = Value::Object(obj);
                    match serde_json::from_value::<FeedEvent>(event_value) {
                        Ok(event) => full_events.push(event),
                        Err(err) => return Err(DxLinkError {
                            error_type: DxLinkErrorType::InvalidMessage,
                            message: format!("Failed to deserialize event: {}", err),
                        }),
                    }
                }

                Ok(full_events)
            }
        }
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
    use crate::feed::{JSONDouble, QuoteEvent};
    use serde_json::Number;

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
            if let FeedEvent::Quote(quote) = &events[0] {
                assert_eq!(quote.event_symbol, "AAPL");
                assert!(matches!(quote.bid_price, Some(JSONDouble::Number(n)) if n == 123.45));
                assert!(matches!(quote.ask_price, Some(JSONDouble::Number(n)) if n == 123.46));
                assert!(matches!(quote.bid_size, Some(JSONDouble::Number(n)) if n == 100.0));
                assert!(matches!(quote.ask_size, Some(JSONDouble::Number(n)) if n == 200.0));
            } else {
                panic!("Expected Quote event");
            }
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
            assert_eq!(data.len(), 1); // Now we have 1 pair of (event_type, values)
            let (event_type, values) = &data[0];
            assert_eq!(event_type, "Quote");
            assert_eq!(values.len(), 6);
            assert_eq!(values[0], "AAPL");
        } else {
            panic!("Expected Compact data format");
        }
    }

    #[test]
    fn test_compact_to_full_conversion_quote() {
        let mut event_fields = FeedEventFields::new();
        event_fields.insert("Quote".to_string(), vec![
            "eventSymbol".to_string(),
            "eventType".to_string(),
            "bidPrice".to_string(),
            "askPrice".to_string(),
            "bidSize".to_string(),
            "askSize".to_string(),
        ]);

        let compact_data = FeedData::Compact(vec![
            ("Quote".to_string(), vec![
                Value::String("AAPL".to_string()),
                Value::String("Quote".to_string()),
                Value::Number(Number::from_f64(123.45).unwrap()),
                Value::Number(Number::from_f64(123.46).unwrap()),
                Value::Number(Number::from_f64(100.0).unwrap()),
                Value::Number(Number::from_f64(200.0).unwrap()),
            ])
        ]);

        let full_events = compact_data.compact_to_full(&event_fields).unwrap();
        assert_eq!(full_events.len(), 1);

        if let FeedEvent::Quote(quote) = &full_events[0] {
            assert_eq!(quote.event_symbol, "AAPL");
            assert!(matches!(quote.bid_price, Some(JSONDouble::Number(n)) if n == 123.45));
            assert!(matches!(quote.ask_price, Some(JSONDouble::Number(n)) if n == 123.46));
            assert!(matches!(quote.bid_size, Some(JSONDouble::Number(n)) if n == 100.0));
            assert!(matches!(quote.ask_size, Some(JSONDouble::Number(n)) if n == 200.0));
        } else {
            panic!("Expected Quote event");
        }
    }

    #[test]
    fn test_compact_to_full_conversion_trade() {
        let mut event_fields = FeedEventFields::new();
        event_fields.insert("Trade".to_string(), vec![
            "eventSymbol".to_string(),
            "eventType".to_string(),
            "price".to_string(),
            "size".to_string(),
            "exchangeCode".to_string(),
        ]);

        let compact_data = FeedData::Compact(vec![
            ("Trade".to_string(), vec![
                Value::String("AAPL".to_string()),
                Value::String("Trade".to_string()),
                Value::Number(Number::from_f64(123.45).unwrap()),
                Value::Number(Number::from_f64(100.0).unwrap()),
                Value::String("XNAS".to_string()),
            ])
        ]);

        let full_events = compact_data.compact_to_full(&event_fields).unwrap();
        assert_eq!(full_events.len(), 1);

        if let FeedEvent::Trade(trade) = &full_events[0] {
            assert_eq!(trade.event_symbol, "AAPL");
            assert!(matches!(trade.price, Some(JSONDouble::Number(n)) if n == 123.45));
            assert!(matches!(trade.size, Some(JSONDouble::Number(n)) if n == 100.0));
            assert_eq!(trade.exchange_code, Some("XNAS".to_string()));
        } else {
            panic!("Expected Trade event");
        }
    }

    #[test]
    fn test_compact_to_full_multiple_events() {
        let mut event_fields = FeedEventFields::new();
        event_fields.insert("Quote".to_string(), vec![
            "eventSymbol".to_string(),
            "eventType".to_string(),
            "bidPrice".to_string(),
            "askPrice".to_string(),
        ]);
        event_fields.insert("Trade".to_string(), vec![
            "eventSymbol".to_string(),
            "eventType".to_string(),
            "price".to_string(),
            "size".to_string(),
        ]);

        let compact_data = FeedData::Compact(vec![
            ("Quote".to_string(), vec![
                Value::String("AAPL".to_string()),
                Value::String("Quote".to_string()),
                Value::Number(Number::from_f64(123.45).unwrap()),
                Value::Number(Number::from_f64(123.46).unwrap()),
            ]),
            ("Trade".to_string(), vec![
                Value::String("AAPL".to_string()),
                Value::String("Trade".to_string()),
                Value::Number(Number::from_f64(123.45).unwrap()),
                Value::Number(Number::from_f64(100.0).unwrap()),
            ])
        ]);

        let full_events = compact_data.compact_to_full(&event_fields).unwrap();
        assert_eq!(full_events.len(), 2);

        // Verify Quote event
        if let FeedEvent::Quote(quote) = &full_events[0] {
            assert_eq!(quote.event_symbol, "AAPL");
            assert!(matches!(quote.bid_price, Some(JSONDouble::Number(n)) if n == 123.45));
            assert!(matches!(quote.ask_price, Some(JSONDouble::Number(n)) if n == 123.46));
        } else {
            panic!("Expected Quote event");
        }

        // Verify Trade event
        if let FeedEvent::Trade(trade) = &full_events[1] {
            assert_eq!(trade.event_symbol, "AAPL");
            assert!(matches!(trade.price, Some(JSONDouble::Number(n)) if n == 123.45));
            assert!(matches!(trade.size, Some(JSONDouble::Number(n)) if n == 100.0));
        } else {
            panic!("Expected Trade event");
        }
    }

    #[test]
    fn test_compact_to_full_missing_fields_config() {
        let event_fields = FeedEventFields::new(); // Empty fields config
        let compact_data = FeedData::Compact(vec![
            ("Quote".to_string(), vec![
                Value::String("AAPL".to_string()),
                Value::String("Quote".to_string()),
                Value::Number(Number::from_f64(123.45).unwrap()),
            ])
        ]);

        let result = compact_data.compact_to_full(&event_fields);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_type, DxLinkErrorType::InvalidMessage);
            assert!(e.message.contains("No field configuration for event type"));
        }
    }

    #[test]
    fn test_compact_to_full_too_many_values() {
        let mut event_fields = FeedEventFields::new();
        event_fields.insert("Quote".to_string(), vec![
            "eventSymbol".to_string(),
            "eventType".to_string(),
            "bidPrice".to_string(),
        ]);

        let compact_data = FeedData::Compact(vec![
            ("Quote".to_string(), vec![
                Value::String("AAPL".to_string()),
                Value::String("Quote".to_string()),
                Value::Number(Number::from_f64(123.45).unwrap()),
                Value::Number(Number::from_f64(123.46).unwrap()), // Extra value
            ])
        ]);

        let result = compact_data.compact_to_full(&event_fields);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_type, DxLinkErrorType::InvalidMessage);
            assert!(e.message.contains("Too many values"));
        }
    }

    #[test]
    fn test_compact_to_full_fewer_values() {
        let mut event_fields = FeedEventFields::new();
        event_fields.insert("Quote".to_string(), vec![
            "eventSymbol".to_string(),
            "eventType".to_string(),
            "bidPrice".to_string(),
            "askPrice".to_string(),
        ]);

        let compact_data = FeedData::Compact(vec![
            ("Quote".to_string(), vec![
                Value::String("AAPL".to_string()),
                Value::String("Quote".to_string()),
                Value::Number(Number::from_f64(123.45).unwrap()),
                // Missing askPrice value
            ])
        ]);

        let full_events = compact_data.compact_to_full(&event_fields).unwrap();
        assert_eq!(full_events.len(), 1);

        if let FeedEvent::Quote(quote) = &full_events[0] {
            assert_eq!(quote.event_symbol, "AAPL");
            assert!(matches!(quote.bid_price, Some(JSONDouble::Number(n)) if n == 123.45));
            assert!(quote.ask_price.is_none()); // Should be None since value was missing
        } else {
            panic!("Expected Quote event");
        }
    }

    #[test]
    fn test_compact_to_full_special_values() {
        let mut event_fields = FeedEventFields::new();
        event_fields.insert("Quote".to_string(), vec![
            "eventSymbol".to_string(),
            "eventType".to_string(),
            "bidPrice".to_string(),
            "askPrice".to_string(),
            "bidSize".to_string(),
        ]);

        let compact_data = FeedData::Compact(vec![
            ("Quote".to_string(), vec![
                Value::String("AAPL".to_string()),
                Value::String("Quote".to_string()),
                Value::String("NaN".to_string()),
                Value::String("Infinity".to_string()),
                Value::String("-Infinity".to_string()),
            ])
        ]);

        let full_events = compact_data.compact_to_full(&event_fields).unwrap();
        assert_eq!(full_events.len(), 1);

        if let FeedEvent::Quote(quote) = &full_events[0] {
            assert_eq!(quote.event_symbol, "AAPL");
            assert!(matches!(quote.bid_price, Some(JSONDouble::NaN)));
            assert!(matches!(quote.ask_price, Some(JSONDouble::Infinity)));
            assert!(matches!(quote.bid_size, Some(JSONDouble::NegInfinity)));
        } else {
            panic!("Expected Quote event");
        }
    }

    #[test]
    fn test_compact_to_full_on_full_data() {
        let quote_event = FeedEvent::Quote(QuoteEvent {
            event_symbol: "AAPL".to_string(),
            event_time: Some(1234567890),
            sequence: Some(0),
            time_nano_part: Some(0),
            bid_time: Some(1234567890),
            bid_exchange_code: None,
            bid_price: Some(JSONDouble::Number(123.45)),
            bid_size: Some(JSONDouble::Number(100.0)),
            ask_time: Some(1234567890),
            ask_exchange_code: None,
            ask_price: Some(JSONDouble::Number(123.46)),
            ask_size: Some(JSONDouble::Number(200.0)),
        });

        let feed_data = FeedData::Full(vec![quote_event.clone()]);
        let event_fields = FeedEventFields::new(); // Empty fields config is fine for Full data

        let result = feed_data.compact_to_full(&event_fields).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], quote_event);
    }

    #[test]
    fn test_feed_setup_serialization_compact() {
        let mut event_fields = FeedEventFields::new();
        event_fields.insert("Quote".to_string(), vec!["bidPrice".to_string(), "askPrice".to_string()]);

        let msg = FeedSetupMessage {
            message_type: "FEED_SETUP".to_string(),
            channel: 1,
            accept_aggregation_period: Some(1000.0),
            accept_data_format: Some(FeedDataFormat::Compact),
            accept_event_fields: Some(event_fields),
        };

        let json = serde_json::to_string(&msg).unwrap();
        println!("Serialized JSON: {}", json);
        assert!(json.contains("\"type\":\"FEED_SETUP\""));
        assert!(json.contains("\"channel\":1"));
        assert!(json.contains("\"acceptAggregationPeriod\":1000.0"));
        assert!(json.contains("\"acceptDataFormat\":\"COMPACT\""));
        assert!(json.contains("\"acceptEventFields\":{\"Quote\":[\"bidPrice\",\"askPrice\"]}"));

        let deserialized: FeedSetupMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.message_type, "FEED_SETUP");
        assert_eq!(deserialized.channel, 1);
        assert_eq!(deserialized.accept_aggregation_period, Some(1000.0));
        assert_eq!(deserialized.accept_data_format, Some(FeedDataFormat::Compact));
        assert!(deserialized.accept_event_fields.is_some());
        let fields = deserialized.accept_event_fields.unwrap();
        assert_eq!(fields.get("Quote").unwrap(), &vec!["bidPrice".to_string(), "askPrice".to_string()]);
    }

    #[test]
    fn test_feed_setup_serialization_no_format() {
        let msg = FeedSetupMessage {
            message_type: "FEED_SETUP".to_string(),
            channel: 1,
            accept_aggregation_period: None,
            accept_data_format: None,
            accept_event_fields: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FEED_SETUP\""));
        assert!(json.contains("\"channel\":1"));
        assert!(!json.contains("acceptAggregationPeriod"));
        assert!(!json.contains("acceptDataFormat"));
        assert!(!json.contains("acceptEventFields"));

        let deserialized: FeedSetupMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.message_type, "FEED_SETUP");
        assert_eq!(deserialized.channel, 1);
        assert_eq!(deserialized.accept_aggregation_period, None);
        assert_eq!(deserialized.accept_data_format, None);
        assert_eq!(deserialized.accept_event_fields, None);
    }

    #[test]
    fn test_feed_config_deserialization_compact() {
        let json = r#"{
            "type": "FEED_CONFIG",
            "channel": 1,
            "aggregationPeriod": 1000.0,
            "dataFormat": "COMPACT",
            "eventFields": {
                "Quote": ["bidPrice", "askPrice"]
            }
        }"#;

        let msg: FeedConfigMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "FEED_CONFIG");
        assert_eq!(msg.channel, 1);
        assert_eq!(msg.aggregation_period, 1000.0);
        assert_eq!(msg.data_format, FeedDataFormat::Compact);
        assert!(msg.event_fields.is_some());
        let fields = msg.event_fields.unwrap();
        assert_eq!(fields.get("Quote").unwrap(), &vec!["bidPrice".to_string(), "askPrice".to_string()]);
    }

    #[test]
    fn test_feed_config_deserialization_full() {
        let json = r#"{
            "type": "FEED_CONFIG",
            "channel": 1,
            "aggregationPeriod": 1000.0,
            "dataFormat": "FULL"
        }"#;

        let msg: FeedConfigMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "FEED_CONFIG");
        assert_eq!(msg.channel, 1);
        assert_eq!(msg.aggregation_period, 1000.0);
        assert_eq!(msg.data_format, FeedDataFormat::Full);
        assert_eq!(msg.event_fields, None);
    }
}
