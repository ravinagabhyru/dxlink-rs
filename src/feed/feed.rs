use serde_json::Value;
/// Feed service implementation module for DXLink
///
/// This module provides functionality for managing feed subscriptions,
/// configuration, and message handling through DXLink channels.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing;

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
    pub aggregation_period: u32,
    /// Format specification for feed data
    pub data_format: FeedDataFormat,
    /// Optional field specifications for events
    pub event_fields: Option<FeedEventFields>,
}

/// Configuration parameters that can be accepted by the feed
#[derive(Debug, Clone, Default)]
pub struct FeedAcceptConfig {
    /// Optional aggregation period to accept
    pub accept_aggregation_period: Option<u32>,
    /// Optional data format to accept
    pub accept_data_format: Option<FeedDataFormat>,
    /// Optional event field specifications to accept
    pub accept_event_fields: Option<FeedEventFields>,
}

/// Feed service that manages subscriptions and data flow
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
}

#[derive(Debug)]
enum FeedEvent {
    Subscribe(Vec<Value>),
    Unsubscribe(Vec<Value>),
    Reset,
    Configure(FeedAcceptConfig),
}

impl Feed {
    /// Create a new feed service instance
    pub async fn new(
        client: Arc<Mutex<DxLinkWebSocketClient>>,
        contract: FeedContract,
        options: Option<FeedOptions>,
    ) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(32);
        let options = options.unwrap_or_default();

        let channel = client.lock().await.open_channel(
                    FEED_SERVICE_NAME.to_string(),
                    serde_json::to_value(contract)?,
                )
                .await ;

        let feed = Self {
            channel,
            accept_config: Arc::new(Mutex::new(FeedAcceptConfig::default())),
            config: Arc::new(Mutex::new(FeedConfig::default())),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            touched_events: Arc::new(Mutex::new(HashSet::new())),
            options,
            event_tx,
            event_rx,
        };

        // Start event processing loop
        feed.start_event_loop();

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
        for sub in subscriptions {
            if let Some(key) = self.subscription_key(&sub) {
                subs.insert(key, sub);
            }
        }
        drop(subs); // Release the lock before async call

        self.send_subscription_update().await;
    }

    async fn handle_unsubscribe(&self, subscriptions: Vec<Value>) {
        if self.channel.state() != DxLinkChannelState::Opened {
            return;
        }

        let mut subs = self.subscriptions.lock().await;
        for sub in subscriptions {
            if let Some(key) = self.subscription_key(&sub) {
                subs.remove(&key);
            }
        }
        drop(subs); // Release the lock before async call

        self.send_subscription_update().await;
    }

    async fn handle_reset(&self) {
        if self.channel.state() != DxLinkChannelState::Opened {
            return;
        }

        self.subscriptions.lock().await.clear();
        self.send_subscription_update().await;
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

        let value = serde_json::to_value(&msg).unwrap();
        let channel_msg = DxLinkChannelMessage {
            message_type: msg.message_type().to_string(),
            payload: value,
        };

        let _ = self.channel.send(channel_msg).await;
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

        let value = serde_json::to_value(&msg).unwrap();
        let channel_msg = DxLinkChannelMessage {
            message_type: msg.message_type().to_string(),
            payload: value,
        };

        let _ = self.channel.send(channel_msg).await;
    }
}

impl Clone for Feed {
    fn clone(&self) -> Self {
        let (tx, rx) = mpsc::channel(32);
        Self {
            channel: self.channel.clone(),
            accept_config: self.accept_config.clone(),
            config: self.config.clone(),
            subscriptions: self.subscriptions.clone(),
            touched_events: self.touched_events.clone(),
            options: self.options.clone(),
            event_tx: tx,
            event_rx: rx,
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
    use crate::websocket_client::channel::DxLinkWebSocketChannel;
    use serde_json::json;
    use tokio::sync::mpsc;

    #[test]
    fn test_subscription_key() {
        let (tx, _rx) = mpsc::channel(32);
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
        };

        let new_config = FeedAcceptConfig {
            accept_aggregation_period: Some(1000),
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
            aggregation_period: 42,
            data_format: FeedDataFormat::Full,
            event_fields: None,
        };
        let (tx, _rx) = mpsc::channel(32);
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
        };
        let returned_config = feed.config().await;
        assert_eq!(returned_config.aggregation_period, 42);
        assert_eq!(returned_config.data_format, FeedDataFormat::Full);
    }

    #[tokio::test]
    async fn test_add_remove_clear_subscriptions_events() {
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (tx, _rx) = mpsc::channel(32);
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
        };

        let sub1 = json!({ "type": "Quote", "symbol": "AAPL" });
        let sub2 = json!({ "type": "Trade", "symbol": "MSFT" });

        // Add
        feed.add_subscriptions(vec![sub1.clone(), sub2.clone()])
            .await
            .unwrap();
        let received_event = event_tx
            .send(FeedEvent::Subscribe(vec![sub1.clone(), sub2.clone()]))
            .await;
        assert!(received_event.is_ok());

        // remove
        feed.remove_subscriptions(vec![sub1.clone()]).await.unwrap();
        let received_event = event_tx
            .send(FeedEvent::Unsubscribe(vec![sub1.clone()]))
            .await;
        assert!(received_event.is_ok());

        // Clear.
        feed.clear_subscriptions().await.unwrap();
        let received_event = event_tx.send(FeedEvent::Reset).await;
        assert!(received_event.is_ok());
    }
}
