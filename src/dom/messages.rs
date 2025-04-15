use serde::{Serialize, Deserialize};
use serde_json::Value;

// Import the Message trait directly to avoid circular dependencies
#[async_trait::async_trait]
pub trait Message: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
    fn message_type(&self) -> &'static str;
    fn channel(&self) -> u64;
    fn payload(&self) -> Value;
}

/// DOM order entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidAskEntry {
    pub price: f64,
    pub size: f64,
}

/// Message for configuring the DOM service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomSetupMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_aggregation_period: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_depth_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_data_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_order_fields: Option<Vec<String>>,
}

impl Message for DomSetupMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "DOM_SETUP"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message with DOM service configuration from server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomConfigMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    #[serde(rename = "aggregationPeriod")]
    pub aggregation_period: u64,
    #[serde(rename = "depthLimit")]
    pub depth_limit: u64,
    #[serde(rename = "dataFormat")]
    pub data_format: String,
    #[serde(rename = "orderFields")]
    pub order_fields: Vec<String>,
}

impl Message for DomConfigMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "DOM_CONFIG"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// Message containing DOM service snapshot data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomSnapshotMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub channel: u64,
    pub time: u64,
    pub bids: Vec<BidAskEntry>,
    pub asks: Vec<BidAskEntry>,
}

impl Message for DomSnapshotMessage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn message_type(&self) -> &'static str {
        "DOM_SNAPSHOT"
    }
    fn channel(&self) -> u64 {
        self.channel
    }
    fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dom_setup_message_serialization() {
        let msg = DomSetupMessage {
            message_type: "DOM_SETUP".to_string(),
            channel: 1,
            accept_aggregation_period: Some(10),
            accept_depth_limit: Some(5),
            accept_data_format: Some("FULL".to_string()),
            accept_order_fields: Some(vec!["price".to_string(), "size".to_string()]),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"DOM_SETUP\""));
        assert!(json.contains("\"channel\":1"));
        assert!(json.contains("\"accept_aggregation_period\":10"));
        assert!(json.contains("\"accept_depth_limit\":5"));
        assert!(json.contains("\"accept_data_format\":\"FULL\""));
        assert!(json.contains("\"accept_order_fields\":[\"price\",\"size\"]"));
    }

    #[test]
    fn test_dom_config_message_deserialization() {
        let json = r#"{
            "type": "DOM_CONFIG",
            "channel": 1,
            "aggregationPeriod": 10,
            "depthLimit": 5,
            "dataFormat": "FULL",
            "orderFields": ["price", "size"]
        }"#;

        let msg: DomConfigMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "DOM_CONFIG");
        assert_eq!(msg.channel, 1);
        assert_eq!(msg.aggregation_period, 10);
        assert_eq!(msg.depth_limit, 5);
        assert_eq!(msg.data_format, "FULL");
        assert_eq!(msg.order_fields.len(), 2);
        assert_eq!(msg.order_fields[0], "price");
        assert_eq!(msg.order_fields[1], "size");
    }

    #[test]
    fn test_dom_snapshot_message_deserialization() {
        let json = r#"{
            "type": "DOM_SNAPSHOT",
            "channel": 1,
            "time": 1710262466056,
            "bids": [
                { "price": 172.33, "size": 376 },
                { "price": 172.32, "size": 381 }
            ],
            "asks": [
                { "price": 172.34, "size": 165 },
                { "price": 172.35, "size": 301 }
            ]
        }"#;

        let msg: DomSnapshotMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "DOM_SNAPSHOT");
        assert_eq!(msg.channel, 1);
        assert_eq!(msg.time, 1710262466056);
        assert_eq!(msg.bids.len(), 2);
        assert_eq!(msg.bids[0].price, 172.33);
        assert_eq!(msg.bids[0].size, 376.0);
        assert_eq!(msg.asks.len(), 2);
        assert_eq!(msg.asks[0].price, 172.34);
        assert_eq!(msg.asks[0].size, 165.0);
    }
}
