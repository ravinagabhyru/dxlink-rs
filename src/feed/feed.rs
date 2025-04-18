use serde_json::Value;
/// Feed service implementation module for DXLink
///
/// This module provides functionality for managing feed subscriptions,
/// configuration, and message handling through DXLink channels.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, broadcast};
use tracing;
use uuid::Uuid;

use super::events::FeedEvent as FeedDataEvent;
use super::messages::*;
use crate::core::channel::{DxLinkChannelMessage, DxLinkChannelState};
use crate::core::errors::{DxLinkError, DxLinkErrorType, Result};
use crate::websocket_client::channel::DxLinkWebSocketChannel;
use crate::websocket_client::DxLinkWebSocketClient;
use crate::websocket_client::messages::Message;

const FEED_SERVICE_NAME: &str = "FEED";
const DEFAULT_BATCH_TIME: u64 = 100;
const DEFAULT_MAX_CHUNK_SIZE: usize = 8192;

/// Configuration options for the feed service
#[derive(Debug, Clone)]
pub struct FeedOptions {
    /// Time in milliseconds to wait for more subscriptions before sending
    pub batch_subscriptions_time: u64,
    /// Maximum size of subscription chunk to send
    pub max_send_subscription_chunk_size: usize,
}

impl Default for FeedOptions {
    fn default() -> Self {
        Self {
            batch_subscriptions_time: DEFAULT_BATCH_TIME,
            max_send_subscription_chunk_size: DEFAULT_MAX_CHUNK_SIZE,
        }
    }
}

/// Main feed service configuration
#[derive(Debug, Clone, Default)]
pub struct FeedConfig {
    /// Time period in milliseconds for data aggregation
    pub aggregation_period: f64,
    /// Format specification for feed data
    pub data_format: FeedDataFormat,
    /// Optional field specifications for events
    pub event_fields: Option<FeedEventFields>,
}

/// Configuration parameters that can be accepted by the feed
#[derive(Debug, Clone, Default)]
pub struct FeedAcceptConfig {
    /// Optional aggregation period to accept
    pub accept_aggregation_period: Option<f64>,
    /// Optional data format to accept
    pub accept_data_format: Option<FeedDataFormat>,
    /// Optional event field specifications to accept
    pub accept_event_fields: Option<FeedEventFields>,
}

/// Feed service that manages subscriptions and data flow
#[allow(unused)]
pub struct Feed {
    /// The underlying channel
    channel: Arc<DxLinkWebSocketChannel>, // Use the concrete type
    /// Current accepted configuration
    accept_config: Arc<Mutex<FeedAcceptConfig>>,
    /// Current active configuration
    config: Arc<Mutex<FeedConfig>>,
    /// Active subscriptions
    subscriptions: Arc<Mutex<HashMap<String, Value>>>,
    /// Event types that have had their schema sent
    touched_events: Arc<Mutex<HashSet<String>>>,
    /// Options for batching and chunking
    options: FeedOptions,
    /// Event sender for subscription changes
    event_tx: mpsc::Sender<FeedEvent>,
    /// Event receiver for subscription changes
    event_rx: mpsc::Receiver<FeedEvent>,
    /// Sender for feed data events
    data_tx: broadcast::Sender<crate::feed::events::FeedEvent>,
    /// Receiver for feed data events
    data_rx: broadcast::Receiver<crate::feed::events::FeedEvent>,
}

#[derive(Debug)]
enum FeedEvent {
    Subscribe(Vec<Value>),
    Unsubscribe(Vec<Value>),
    Reset,
    Configure(FeedAcceptConfig),
}

#[allow(unused)]
impl Feed {
    /// Create a new feed service instance
    pub async fn new(
        client: Arc<Mutex<DxLinkWebSocketClient>>,
        contract: FeedContract,
        options: Option<FeedOptions>,
        data_format: Option<FeedDataFormat>,
    ) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        let options = options.unwrap_or_default();

        let parameters = serde_json::to_value(FeedParameters { contract })?;
        let channel = client.lock().await.open_channel(
                    FEED_SERVICE_NAME.to_string(),
                    parameters,
                )
                .await ;

        // Setup message listeners
        let config_clone = Arc::new(Mutex::new(FeedConfig::default()));
        let config_ref = config_clone.clone();
        let data_tx_clone = data_tx.clone();

        // Channel message listener - reads message type from DxLinkChannelMessage
        channel.add_message_listener(Box::new(move |message| {
            // In the websocket client, message is a DxLinkChannelMessage
            // Extract type and payload from it
            if let Ok(value) = serde_json::from_value::<serde_json::Value>(message.payload.clone()) {
                if let Some(type_value) = value.get("type") {
                    if let Some(type_str) = type_value.as_str() {
                        if type_str == "FEED_CONFIG" {
                            // Parse the message payload as FeedConfigMessage
                            if let Ok(feed_config) = serde_json::from_value::<FeedConfigMessage>(message.payload.clone()) {
                                tokio::spawn({
                                    let config_ref = config_ref.clone();
                                    async move {
                                        let new_config = FeedConfig {
                                            aggregation_period: feed_config.aggregation_period,
                                            data_format: feed_config.data_format,
                                            event_fields: feed_config.event_fields.clone(),
                                        };
                                        *config_ref.lock().await = new_config;
                                    }
                                });
                            }
                        } else if type_str == "FEED_DATA" {
                            // Parse the message payload as FeedDataMessage
                            if let Ok(feed_data) = serde_json::from_value::<FeedDataMessage>(message.payload.clone()) {
                                tokio::spawn({
                                    let data_tx = data_tx_clone.clone();
                                    let config_ref = config_ref.clone();
                                    async move {
                                        let config = config_ref.lock().await.clone();
                                        match &feed_data.data {
                                            FeedData::Full(events) => {
                                                // Forward each event to listeners
                                                for event in events {
                                                    let _ = data_tx.send(event.clone());
                                                }
                                            },
                                            FeedData::Compact(_data) => {
                                                // Process compact data if we have event fields configuration
                                                if let Some(ref fields) = config.event_fields {
                                                    match feed_data.data.compact_to_full(fields) {
                                                        Ok(events) => {
                                                            for event in events {
                                                                let _ = data_tx.send(event);
                                                            }
                                                        },
                                                        Err(e) => {
                                                            tracing::error!("Failed to convert compact data to full format: {}", e);
                                                        }
                                                    }
                                                } else {
                                                    tracing::error!("Received compact data but no event fields configuration available");
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                        }
                    }
                }
            }
        }));

        let feed = Self {
            channel,
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig {
                accept_aggregation_period: None,
                accept_data_format: data_format,
                accept_event_fields: None,
            })),
            config: config_clone,
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options,
            event_tx,
            event_rx,
            data_tx,
            data_rx,
        };

        // Start event processing loop
        feed.start_event_loop();

        // Send initial setup message with the requested data format
        feed.send_accept_config().await;

        Ok(feed)
    }

    /// Configure the feed service
    pub async fn configure(&self, config: FeedAcceptConfig) -> Result<()> {
        self.event_tx.send(FeedEvent::Configure(config)).await?;
        Ok(())
    }

    /// Add subscriptions
    pub async fn add_subscriptions(&self, subscriptions: Vec<Value>) -> Result<()> {
        self.event_tx
            .send(FeedEvent::Subscribe(subscriptions))
            .await?;
        Ok(())
    }

    /// Remove subscriptions
    pub async fn remove_subscriptions(&self, subscriptions: Vec<Value>) -> Result<()> {
        self.event_tx
            .send(FeedEvent::Unsubscribe(subscriptions))
            .await?;
        Ok(())
    }

    /// Clear all subscriptions
    pub async fn clear_subscriptions(&self) -> Result<()> {
        self.event_tx.send(FeedEvent::Reset).await?;
        Ok(())
    }

    /// Get current channel state
    pub fn state(&self) -> DxLinkChannelState {
        self.channel.state()
    }

    /// Get current feed configuration
    pub async fn config(&self) -> FeedConfig {
        self.config.lock().await.clone()
    }

    /// Close the feed service
    pub async fn close(&mut self) {
        let _ = self.channel.close().await;
    }

    /// Add a listener for feed data events
    ///
    /// Returns a UUID that can be used to remove the listener
    pub fn add_data_listener<F>(&self, listener: F) -> Uuid
    where
        F: Fn(FeedDataEvent) + Send + Sync + 'static,
    {
        let listener_id = Uuid::new_v4();
        let mut rx = self.data_tx.subscribe();

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                listener(event);
            }
        });

        listener_id
    }

    /// Register for specific types of feed events
    pub fn subscribe_to_data_events(&self) -> broadcast::Receiver<FeedDataEvent> {
        self.data_tx.subscribe()
    }

    // Private implementation details

    fn start_event_loop(&self) {
        let feed = self.clone();
        tokio::spawn(async move {
            if let Err(e) = feed.process_events().await {
                tracing::error!("Event processing loop failed: {}", e);
            }
        });
    }

    async fn process_events(mut self) -> Result<()> {
        while let Some(event) = self.event_rx.recv().await {
            match event {
                FeedEvent::Subscribe(subs) => {
                    self.handle_subscribe(subs).await;
                }
                FeedEvent::Unsubscribe(subs) => {
                    self.handle_unsubscribe(subs).await;
                }
                FeedEvent::Reset => {
                    self.handle_reset().await;
                }
                FeedEvent::Configure(config) => {
                    self.handle_configure(config).await;
                }
            }
        }
        Ok(())
    }

    async fn handle_subscribe(&self, subscriptions: Vec<Value>) {
        if self.channel.state() != DxLinkChannelState::Opened {
            return;
        }

        let mut subs = self.subscriptions.lock().await;
        // Track which subscriptions were added
        let mut added = Vec::new();
        for sub in subscriptions {
            if let Some(key) = self.subscription_key(&sub) {
                subs.insert(key, sub.clone());
                added.push(sub);
            }
        }
        drop(subs); // Release the lock before async call

        // Send only the added subscriptions without reset flag
        if !added.is_empty() {
            self.send_subscription_add(added).await;
        }
    }

    async fn handle_unsubscribe(&self, subscriptions: Vec<Value>) {
        if self.channel.state() != DxLinkChannelState::Opened {
            return;
        }

        let mut subs = self.subscriptions.lock().await;
        // Track which subscriptions were removed
        let mut removed = Vec::new();
        for sub in subscriptions {
            if let Some(key) = self.subscription_key(&sub) {
                if subs.remove(&key).is_some() {
                    removed.push(sub);
                }
            }
        }
        drop(subs); // Release the lock before async call

        // Send only the removed subscriptions without reset flag
        if !removed.is_empty() {
            self.send_subscription_remove(removed).await;
        }
    }

    async fn handle_reset(&self) {
        if self.channel.state() != DxLinkChannelState::Opened {
            return;
        }

        self.subscriptions.lock().await.clear();
        self.send_subscription_reset().await;
    }

    async fn handle_configure(&self, config: FeedAcceptConfig) {
        *self.accept_config.lock().await = config;
        if self.channel.state() == DxLinkChannelState::Opened {
            self.send_accept_config().await;
        }
    }

    fn subscription_key(&self, sub: &Value) -> Option<String> {
        let obj = sub.as_object()?;
        let type_str = obj.get("type")?.as_str()?;
        let symbol = obj.get("symbol")?.as_str()?;
        let source = obj.get("source").and_then(|s| s.as_str());

        Some(match source {
            Some(src) => format!("{}#{}:{}", type_str, src, symbol),
            None => format!("{}:{}", type_str, symbol),
        })
    }

    async fn send_subscription_add(&self, subscriptions: Vec<Value>) {
        if subscriptions.is_empty() {
            return;
        }

        // Convert from Value to FeedSubscriptionEntry if needed
        let entries: Vec<FeedSubscriptionEntry> = subscriptions
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();

        if entries.is_empty() {
            return;
        }

        let msg = FeedSubscriptionMessage {
            message_type: "FEED_SUBSCRIPTION".into(),
            channel: self.channel.id,
            add: Some(entries),
            remove: None,
            reset: None,
        };

        // Convert to DxLinkChannelMessage format expected by channel.send
        let channel_msg = DxLinkChannelMessage {
            message_type: msg.message_type().to_string(),
            payload: serde_json::to_value(&msg).unwrap(),
        };

        if let Err(e) = self.channel.send(channel_msg).await {
            tracing::error!("Failed to send subscription add: {}", e);
        }
    }

    async fn send_subscription_remove(&self, subscriptions: Vec<Value>) {
        if subscriptions.is_empty() {
            return;
        }

        // Convert from Value to FeedSubscriptionEntry if needed
        let entries: Vec<FeedSubscriptionEntry> = subscriptions
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();

        if entries.is_empty() {
            return;
        }

        let msg = FeedSubscriptionMessage {
            message_type: "FEED_SUBSCRIPTION".into(),
            channel: self.channel.id,
            add: None,
            remove: Some(entries),
            reset: None,
        };

        // Convert to DxLinkChannelMessage format expected by channel.send
        let channel_msg = DxLinkChannelMessage {
            message_type: msg.message_type().to_string(),
            payload: serde_json::to_value(&msg).unwrap(),
        };

        if let Err(e) = self.channel.send(channel_msg).await {
            tracing::error!("Failed to send subscription remove: {}", e);
        }
    }

    async fn send_subscription_reset(&self) {
        let msg = FeedSubscriptionMessage {
            message_type: "FEED_SUBSCRIPTION".into(),
            channel: self.channel.id,
            add: None,
            remove: None,
            reset: Some(true),
        };

        // Convert to DxLinkChannelMessage format expected by channel.send
        let channel_msg = DxLinkChannelMessage {
            message_type: msg.message_type().to_string(),
            payload: serde_json::to_value(&msg).unwrap(),
        };

        if let Err(e) = self.channel.send(channel_msg).await {
            tracing::error!("Failed to send subscription reset: {}", e);
        }
    }

    // Legacy method for backward compatibility
    async fn send_subscription_update(&self) {
        let subs = self.subscriptions.lock().await;
        // Convert from Value to FeedSubscriptionEntry if needed
        let entries: Vec<FeedSubscriptionEntry> = subs
            .values()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();

        let msg = FeedSubscriptionMessage {
            message_type: "FEED_SUBSCRIPTION".into(),
            channel: self.channel.id,
            add: if entries.is_empty() {
                None
            } else {
                Some(entries)
            },
            remove: None,
            reset: Some(true),
        };

        // Convert to DxLinkChannelMessage format expected by channel.send
        let channel_msg = DxLinkChannelMessage {
            message_type: msg.message_type().to_string(),
            payload: serde_json::to_value(&msg).unwrap(),
        };

        if let Err(e) = self.channel.send(channel_msg).await {
            tracing::error!("Failed to send subscription update: {}", e);
        }
    }

    async fn send_accept_config(&self) {
        let config = self.accept_config.lock().await;
        let msg = FeedSetupMessage {
            message_type: "FEED_SETUP".into(),
            channel: self.channel.id,
            accept_aggregation_period: config.accept_aggregation_period,
            accept_data_format: config.accept_data_format,
            accept_event_fields: config.accept_event_fields.clone(),
        };

        // Convert to DxLinkChannelMessage format expected by channel.send
        let channel_msg = DxLinkChannelMessage {
            message_type: msg.message_type().to_string(),
            payload: serde_json::to_value(&msg).unwrap(),
        };

        if let Err(e) = self.channel.send(channel_msg).await {
            tracing::error!("Failed to send accept config: {}", e);
        }
    }

    /// Handle an incoming feed configuration message
    async fn handle_feed_config(&self, message: FeedConfigMessage) {
        let new_config = FeedConfig {
            aggregation_period: message.aggregation_period,
            data_format: message.data_format,
            event_fields: message.event_fields.clone(),
        };

        // Log format change if it's different
        {
            let current = self.config.lock().await;
            if current.data_format != new_config.data_format {
                tracing::info!(
                    "Feed data format changed from {:?} to {:?}",
                    current.data_format,
                    new_config.data_format
                );
            }
        }

        // Update config
        *self.config.lock().await = new_config;
    }

    /// Handle an incoming feed data message
    async fn handle_feed_data(&self, message: FeedDataMessage) {
        let config = self.config.lock().await.clone();

        match &message.data {
            FeedData::Full(events) => {
                // Forward each event to listeners
                for event in events {
                    let _ = self.data_tx.send(event.clone());
                }
            },
            FeedData::Compact(_data) => {
                // Process compact data if we have event fields configuration
                if let Some(ref fields) = config.event_fields {
                    match message.data.compact_to_full(fields) {
                        Ok(events) => {
                            for event in events {
                                let _ = self.data_tx.send(event);
                            }
                        },
                        Err(e) => {
                            tracing::error!("Failed to convert compact data to full format: {}", e);
                        }
                    }
                } else {
                    tracing::error!("Received compact data but no event fields configuration available");
                }
            }
        }
    }
}

impl Clone for Feed {
    fn clone(&self) -> Self {
        let (tx, rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        Self {
            channel: self.channel.clone(),
            accept_config: self.accept_config.clone(),
            config: self.config.clone(),
            subscriptions: self.subscriptions.clone(),
            touched_events: self.touched_events.clone(),
            options: self.options.clone(),
            event_tx: tx,
            event_rx: rx,
            data_tx,
            data_rx
        }
    }
}

impl From<mpsc::error::SendError<FeedEvent>> for DxLinkError {
    fn from(err: mpsc::error::SendError<FeedEvent>) -> Self {
        DxLinkError {
            error_type: DxLinkErrorType::BadAction,
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::events::{FeedEvent as FeedDataEvent, QuoteEvent, JSONDouble};
    use crate::websocket_client::config::DxLinkWebSocketClientConfig;
    use crate::websocket_client::channel::DxLinkWebSocketChannel;
    use serde_json::json;
    use tokio::sync::mpsc;

    #[test]
    fn test_subscription_key() {
        let (tx, _rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        let feed = Feed {
            channel: Arc::new(DxLinkWebSocketChannel::new(
                0,
                "test".to_string(),
                serde_json::json!({}),
                tx,
                &crate::websocket_client::config::DxLinkWebSocketClientConfig::default(),
            )), // Use the concrete channel
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig::default())),
            config: Arc::new(Mutex::new(FeedConfig::default())),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options: FeedOptions::default(),
            event_tx: tokio::sync::mpsc::channel(32).0,
            event_rx: tokio::sync::mpsc::channel(32).1,
            data_tx,
            data_rx,
        };

        let sub1 = json!({ "type": "Quote", "symbol": "AAPL" });
        let key1 = feed.subscription_key(&sub1);
        assert_eq!(key1, Some("Quote:AAPL".to_string()));

        let sub2 = json!({ "type": "Trade", "symbol": "MSFT", "source": "NYSE" });
        let key2 = feed.subscription_key(&sub2);
        assert_eq!(key2, Some("Trade#NYSE:MSFT".to_string()));

        let sub3 = json!({ "type": "Quote", "symbol": "GOOG" }); // Test again with the first parameters.
        let key3 = feed.subscription_key(&sub3);
        assert_eq!(key3, Some("Quote:GOOG".to_string()));
    }

    // Test handle_configure (no mock needed)
    #[tokio::test]
    async fn test_handle_configure_internal() {
        let (event_tx, event_rx) = mpsc::channel(32);
        let (tx, _rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        let feed = Feed {
            channel: Arc::new(DxLinkWebSocketChannel::new(
                0,
                "test".to_string(),
                serde_json::json!({}),
                tx,
                &crate::websocket_client::config::DxLinkWebSocketClientConfig::default(),
            )), // Use the concrete channel
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig::default())),
            config: Arc::new(Mutex::new(FeedConfig::default())),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options: FeedOptions::default(),
            event_tx,
            event_rx,
            data_tx,
            data_rx,
        };

        let new_config = FeedAcceptConfig {
            accept_aggregation_period: Some(1000.0),
            accept_data_format: Some(FeedDataFormat::Full),
            accept_event_fields: None,
        };

        feed.handle_configure(new_config.clone()).await;
        assert_eq!(
            feed.accept_config.lock().await.accept_aggregation_period,
            new_config.accept_aggregation_period
        );
        assert_eq!(
            feed.accept_config.lock().await.accept_data_format,
            new_config.accept_data_format
        );
    }
    // Test config()
    #[tokio::test]
    async fn test_config() {
        let (event_tx, event_rx) = mpsc::channel(32);
        let initial_config = FeedConfig {
            aggregation_period: 42.0,
            data_format: FeedDataFormat::Full,
            event_fields: None,
        };
        let (tx, _rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        let feed = Feed {
            channel: Arc::new(DxLinkWebSocketChannel::new(
                0,
                "test".to_string(),
                serde_json::json!({}),
                tx,
                &crate::websocket_client::config::DxLinkWebSocketClientConfig::default(),
            )), // Use the concrete channel
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig::default())),
            config: Arc::new(Mutex::new(initial_config.clone())),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options: FeedOptions::default(),
            event_tx,
            event_rx,
            data_tx,
            data_rx
        };
        let returned_config = feed.config().await;
        assert_eq!(returned_config.aggregation_period, 42.0);
        assert_eq!(returned_config.data_format, FeedDataFormat::Full);
    }

    #[tokio::test]
    async fn test_add_remove_clear_subscriptions_events() {
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (tx, _rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        let feed = Feed {
            channel: Arc::new(DxLinkWebSocketChannel::new(
                0,
                "test".to_string(),
                serde_json::json!({}),
                tx,
                &crate::websocket_client::config::DxLinkWebSocketClientConfig::default(),
            )), // Use the concrete channel
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig::default())),
            config: Arc::new(Mutex::new(FeedConfig::default())),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options: FeedOptions::default(),
            event_tx: event_tx.clone(),
            event_rx: tokio::sync::mpsc::channel(32).1, // Don't use the real receiver
            data_tx,
            data_rx
        };

        let sub1 = json!({ "type": "Quote", "symbol": "AAPL" });
        let sub2 = json!({ "type": "Trade", "symbol": "MSFT" });

        // Add
        feed.add_subscriptions(vec![sub1.clone(), sub2.clone()])
            .await
            .unwrap();
        let received_event = event_tx
            .send(super::FeedEvent::Subscribe(vec![sub1.clone(), sub2.clone()]))
            .await;
        assert!(received_event.is_ok());

        // remove
        feed.remove_subscriptions(vec![sub1.clone()]).await.unwrap();
        let received_event = event_tx
            .send(super::FeedEvent::Unsubscribe(vec![sub1.clone()]))
            .await;
        assert!(received_event.is_ok());

        // Clear.
        feed.clear_subscriptions().await.unwrap();
        let received_event = event_tx.send(super::FeedEvent::Reset).await;
        assert!(received_event.is_ok());
    }

    #[tokio::test]
    async fn test_feed_new_requests_compact() {
        let client = Arc::new(Mutex::new(DxLinkWebSocketClient::new(
            DxLinkWebSocketClientConfig::default(),
        )));

        let feed = Feed::new(
            client.clone(),
            FeedContract::Stream,
            None,
            Some(FeedDataFormat::Compact),
        ).await.unwrap();

        // Verify the accept_config was set correctly
        let accept_config = feed.accept_config.lock().await;
        assert_eq!(accept_config.accept_data_format, Some(FeedDataFormat::Compact));
    }

    #[tokio::test]
    async fn test_feed_new_requests_default_full() {
        let client = Arc::new(Mutex::new(DxLinkWebSocketClient::new(
            DxLinkWebSocketClientConfig::default(),
        )));

        let feed = Feed::new(
            client.clone(),
            FeedContract::Stream,
            None,
            None,
        ).await.unwrap();

        // Verify the accept_config was set correctly (default should be None)
        let accept_config = feed.accept_config.lock().await;
        assert_eq!(accept_config.accept_data_format, None);
    }

    #[tokio::test]
    async fn test_feed_handles_config_message_compact() {
        let (tx, _rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        let feed = Feed {
            channel: Arc::new(DxLinkWebSocketChannel::new(
                0,
                "test".to_string(),
                serde_json::json!({}),
                tx,
                &DxLinkWebSocketClientConfig::default(),
            )),
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig::default())),
            config: Arc::new(Mutex::new(FeedConfig::default())),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options: FeedOptions::default(),
            event_tx: tokio::sync::mpsc::channel(32).0,
            event_rx: tokio::sync::mpsc::channel(32).1,
            data_tx,
            data_rx,
        };

        // Create a config message with Compact format and fields
        let config_msg = FeedConfigMessage {
            message_type: "FEED_CONFIG".to_string(),
            channel: 0,
            aggregation_period: 1000.0,
            data_format: FeedDataFormat::Compact,
            event_fields: Some(HashMap::from([
                ("Quote".to_string(), vec!["bidPrice".to_string(), "askPrice".to_string()]),
            ])),
        };

        // Handle the config message
        feed.handle_feed_config(config_msg.clone()).await;

        // Verify config was updated correctly
        let config = feed.config.lock().await;
        assert_eq!(config.data_format, FeedDataFormat::Compact);
        assert_eq!(config.aggregation_period, 1000.0);
        assert!(config.event_fields.is_some());
        let fields = config.event_fields.as_ref().unwrap();
        assert_eq!(
            fields.get("Quote").unwrap(),
            &vec!["bidPrice".to_string(), "askPrice".to_string()]
        );
    }

    #[tokio::test]
    async fn test_feed_handles_config_message_full() {
        let (tx, _rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        let feed = Feed {
            channel: Arc::new(DxLinkWebSocketChannel::new(
                0,
                "test".to_string(),
                serde_json::json!({}),
                tx,
                &DxLinkWebSocketClientConfig::default(),
            )),
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig::default())),
            config: Arc::new(Mutex::new(FeedConfig::default())),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options: FeedOptions::default(),
            event_tx: tokio::sync::mpsc::channel(32).0,
            event_rx: tokio::sync::mpsc::channel(32).1,
            data_tx,
            data_rx,
        };

        // Create a config message with Full format
        let config_msg = FeedConfigMessage {
            message_type: "FEED_CONFIG".to_string(),
            channel: 0,
            aggregation_period: 500.0,
            data_format: FeedDataFormat::Full,
            event_fields: None,
        };

        // Handle the config message
        feed.handle_feed_config(config_msg.clone()).await;

        // Verify config was updated correctly
        let config = feed.config.lock().await;
        assert_eq!(config.data_format, FeedDataFormat::Full);
        assert_eq!(config.aggregation_period, 500.0);
        assert!(config.event_fields.is_none());
    }

    #[tokio::test]
    async fn test_feed_processes_full_data_when_full_config() {
        let (tx, _rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        let feed = Feed {
            channel: Arc::new(DxLinkWebSocketChannel::new(
                0,
                "test".to_string(),
                serde_json::json!({}),
                tx,
                &DxLinkWebSocketClientConfig::default(),
            )),
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig::default())),
            config: Arc::new(Mutex::new(FeedConfig {
                aggregation_period: 0.0,
                data_format: FeedDataFormat::Full,
                event_fields: None,
            })),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options: FeedOptions::default(),
            event_tx: tokio::sync::mpsc::channel(32).0,
            event_rx: tokio::sync::mpsc::channel(32).1,
            data_tx: data_tx.clone(),
            data_rx,
        };

        // Create a full format data message
        let data_msg = FeedDataMessage {
            message_type: "FEED_DATA".to_string(),
            channel: 0,
            data: FeedData::Full(vec![
                FeedDataEvent::Quote(QuoteEvent {
                    event_symbol: "AAPL".to_string(),
                    event_time: Some(1234567890),
                    sequence: None,
                    time_nano_part: None,
                    bid_time: None,
                    bid_exchange_code: None,
                    bid_price: Some(JSONDouble::Number(150.0)),
                    bid_size: Some(JSONDouble::Number(100.0)),
                    ask_time: None,
                    ask_exchange_code: None,
                    ask_price: Some(JSONDouble::Number(151.0)),
                    ask_size: Some(JSONDouble::Number(200.0)),
                })
            ]),
        };

        // Subscribe to events before sending data
        let mut rx = data_tx.subscribe();

        // Handle the data message
        feed.handle_feed_data(data_msg).await;

        // Use timeout for receiving the event
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rx.recv()
        ).await.expect("Timeout waiting for event").expect("Failed to receive event");

        match event {
            FeedDataEvent::Quote(quote) => {
                assert_eq!(quote.event_symbol, "AAPL");
                assert!(matches!(quote.bid_price, Some(JSONDouble::Number(n)) if n == 150.0));
                assert!(matches!(quote.ask_price, Some(JSONDouble::Number(n)) if n == 151.0));
            },
            _ => panic!("Expected Quote event"),
        }
    }

    #[tokio::test]
    async fn test_feed_processes_compact_data_when_compact_config() {
        let (tx, _rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        let feed = Feed {
            channel: Arc::new(DxLinkWebSocketChannel::new(
                0,
                "test".to_string(),
                serde_json::json!({}),
                tx,
                &DxLinkWebSocketClientConfig::default(),
            )),
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig::default())),
            config: Arc::new(Mutex::new(FeedConfig {
                aggregation_period: 0.0,
                data_format: FeedDataFormat::Compact,
                event_fields: Some(HashMap::from([
                    ("Quote".to_string(), vec![
                        "eventSymbol".to_string(),
                        "eventTime".to_string(),
                        "bidPrice".to_string(),
                        "askPrice".to_string(),
                    ]),
                ])),
            })),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options: FeedOptions::default(),
            event_tx: tokio::sync::mpsc::channel(32).0,
            event_rx: tokio::sync::mpsc::channel(32).1,
            data_tx: data_tx.clone(),
            data_rx,
        };

        // Create a compact format data message
        let data_msg = FeedDataMessage {
            message_type: "FEED_DATA".to_string(),
            channel: 0,
            data: FeedData::Compact(vec![
                ("Quote".to_string(), vec![
                    serde_json::Value::String("AAPL".to_string()),
                    serde_json::Value::Number(1234567890.into()),
                    serde_json::Value::Number(150.into()),
                    serde_json::Value::Number(151.into()),
                ]),
            ]),
        };

        // Subscribe to events before sending data
        let mut rx = data_tx.subscribe();

        // Handle the data message
        feed.handle_feed_data(data_msg).await;

        // Use timeout for receiving the event
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rx.recv()
        ).await.expect("Timeout waiting for event").expect("Failed to receive event");

        match event {
            FeedDataEvent::Quote(quote) => {
                assert_eq!(quote.event_symbol, "AAPL");
                assert_eq!(quote.event_time, Some(1234567890));
                assert!(matches!(quote.bid_price, Some(JSONDouble::Number(n)) if n == 150.0));
                assert!(matches!(quote.ask_price, Some(JSONDouble::Number(n)) if n == 151.0));
            },
            _ => panic!("Expected Quote event"),
        }
    }

    #[tokio::test]
    async fn test_feed_errors_on_compact_data_without_fields() {
        let (tx, _rx) = mpsc::channel(32);
        let (data_tx, data_rx) = broadcast::channel(100);
        let feed = Feed {
            channel: Arc::new(DxLinkWebSocketChannel::new(
                0,
                "test".to_string(),
                serde_json::json!({}),
                tx,
                &DxLinkWebSocketClientConfig::default(),
            )),
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig::default())),
            config: Arc::new(Mutex::new(FeedConfig {
                aggregation_period: 0.0,
                data_format: FeedDataFormat::Compact,
                event_fields: None,
            })),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options: FeedOptions::default(),
            event_tx: tokio::sync::mpsc::channel(32).0,
            event_rx: tokio::sync::mpsc::channel(32).1,
            data_tx,
            data_rx,
        };

        // Create a compact format data message
        let data_msg = FeedDataMessage {
            message_type: "FEED_DATA".to_string(),
            channel: 0,
            data: FeedData::Compact(vec![
                ("Quote".to_string(), vec![
                    serde_json::Value::String("AAPL".to_string()),
                    serde_json::Value::Number(150.into()),
                    serde_json::Value::Number(151.into()),
                ]),
            ]),
        };

        // Handle the data message
        feed.handle_feed_data(data_msg).await;

        // Verify no events were broadcast
        let mut rx = feed.data_tx.subscribe();
        assert!(rx.try_recv().is_err());
    }
}
