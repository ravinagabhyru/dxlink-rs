//! DOM (Depth of Market) service implementation for dxLink protocol
//!
//! This module provides functionality for managing depth of market data, including:
//! - State management for DOM data
//! - Handling of DOM-specific messages (setup, config, snapshot)
//! - Synchronization of market depth updates
//!
//! # Examples
//!
//! ```rust
//! use dxlink_rs::dom::{DomService, DomSetupMessage};
//! use tokio::sync::mpsc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let (tx, _rx) = mpsc::channel(32);
//!     let service = DomService::new(tx);
//!
//!     // Initialize with default configuration
//!     service.initialize(1, None).await.unwrap();
//!
//!     // Get current state
//!     let state = service.get_state().await;
//!     println!("Current DOM state: {:?}", state);
//! }
//! ```

use crate::core::{errors::ChannelError, Result};
use crate::dom::messages::{
    BidAskEntry, DomConfigMessage, DomSetupMessage, DomSnapshotMessage, Message,
};
use std::convert::TryFrom;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Represents the state of a DOM (Depth of Market) service
///
/// This struct holds the current configuration and market data for a DOM service,
/// including bid and ask orders at different price levels.
#[derive(Debug, Clone)]
pub struct DomState {
    /// Channel ID for this DOM service
    pub channel: u64,
    /// Aggregation period in milliseconds
    pub aggregation_period: u64,
    /// Maximum depth of market to maintain
    pub depth_limit: u64,
    /// Data format for order fields
    pub data_format: String,
    /// List of order fields to include in updates
    pub order_fields: Vec<String>,
    /// Current bid orders, sorted by price (highest first)
    pub bids: Vec<BidAskEntry>,
    /// Current ask orders, sorted by price (lowest first)
    pub asks: Vec<BidAskEntry>,
    /// Last snapshot timestamp provided by server
    pub last_update_time: u64,
    /// Optional sequence value provided by the protocol
    pub sequence: Option<u64>,
}

impl Default for DomState {
    fn default() -> Self {
        Self {
            channel: 0,
            aggregation_period: 1000,
            depth_limit: 10,
            data_format: "FULL".to_string(),
            order_fields: vec!["price".to_string(), "size".to_string()],
            bids: Vec::new(),
            asks: Vec::new(),
            last_update_time: 0,
            sequence: None,
        }
    }
}

impl DomState {
    pub fn to_setup_message(&self) -> DomSetupMessage {
        DomSetupMessage {
            message_type: "DOM_SETUP".to_string(),
            channel: self.channel,
            accept_aggregation_period: Some(self.aggregation_period),
            accept_depth_limit: Some(self.depth_limit),
            accept_data_format: Some(self.data_format.clone()),
            accept_order_fields: Some(self.order_fields.clone()),
        }
    }
}

/// DOM service that manages depth of market data
///
/// This service handles the lifecycle of DOM data, including:
/// - Initialization and configuration
/// - Processing of DOM-specific messages
/// - Maintaining current market state
///
/// # Examples
///
/// ```rust
/// use dxlink_rs::dom::{DomService, DomSetupMessage};
/// use tokio::sync::mpsc;
///
/// #[tokio::main]
/// async fn main() {
///     let (tx, _rx) = mpsc::channel(32);
///     let service = DomService::new(tx);
///
///     // Initialize with custom configuration
///     let config = DomSetupMessage {
///         message_type: "DOM_SETUP".to_string(),
///         channel: 1,
///         accept_aggregation_period: Some(500),
///         accept_depth_limit: Some(20),
///         accept_data_format: Some("FULL".to_string()),
///         accept_order_fields: Some(vec!["price".to_string(), "size".to_string()]),
///     };
///     service.initialize(1, Some(config)).await.unwrap();
/// }
/// ```
pub struct DomService {
    state: Arc<RwLock<DomState>>,
    tx: mpsc::Sender<Box<dyn Message>>,
}

impl DomService {
    /// Create a new DOM service instance
    ///
    /// # Arguments
    ///
    /// * `tx` - Sender channel for outgoing messages
    ///
    /// # Returns
    ///
    /// A new `DomService` instance with default configuration
    pub fn new(tx: mpsc::Sender<Box<dyn Message>>) -> Self {
        Self {
            state: Arc::new(RwLock::new(DomState::default())),
            tx,
        }
    }

    /// Initialize the DOM service with configuration
    ///
    /// # Arguments
    ///
    /// * `channel` - Channel ID for this DOM service
    /// * `config` - Optional custom configuration for the service
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure of the initialization
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The setup message cannot be sent
    /// - The channel ID is invalid
    pub async fn initialize(&self, channel: u64, config: Option<DomSetupMessage>) -> Result<()> {
        if channel == 0 {
            return Err(ChannelError::InvalidChannel {
                channel,
                reason: "channel id must be greater than zero".to_string(),
            }
            .into());
        }

        let mut state = self.state.write().await;

        if state.channel != 0 && state.channel != channel {
            return Err(ChannelError::InvalidChannel {
                channel,
                reason: format!("service already bound to channel {}", state.channel),
            }
            .into());
        }

        state.channel = channel;

        let mut setup_msg = if let Some(mut provided) = config {
            if provided.channel == 0 {
                provided.channel = channel;
            } else if provided.channel != channel {
                return Err(ChannelError::InvalidChannel {
                    channel: provided.channel,
                    reason: format!(
                        "setup message channel does not match requested channel {}",
                        channel
                    ),
                }
                .into());
            }

            provided.message_type = "DOM_SETUP".to_string();

            if let Some(period) = provided.accept_aggregation_period {
                state.aggregation_period = period;
            }
            if let Some(limit) = provided.accept_depth_limit {
                state.depth_limit = limit;
            }
            if let Some(ref format) = provided.accept_data_format {
                state.data_format = format.clone();
            }
            if let Some(ref fields) = provided.accept_order_fields {
                state.order_fields = fields.clone();
            }

            provided.channel = channel;
            provided
        } else {
            state.to_setup_message()
        };

        setup_msg.channel = channel;
        state.last_update_time = 0;
        state.sequence = None;
        state.bids.clear();
        state.asks.clear();

        self.tx
            .send(Box::new(setup_msg))
            .await
            .map_err(|e| ChannelError::SendError(e.to_string()))?;

        Ok(())
    }

    /// Handle incoming DOM configuration message
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration message containing new settings
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure of the configuration update
    pub async fn handle_config(&self, config: DomConfigMessage) -> Result<()> {
        if config.channel == 0 {
            return Err(ChannelError::InvalidChannel {
                channel: config.channel,
                reason: "config channel must be greater than zero".to_string(),
            }
            .into());
        }

        let mut state = self.state.write().await;

        if state.channel != 0 && state.channel != config.channel {
            return Err(ChannelError::InvalidChannel {
                channel: config.channel,
                reason: format!(
                    "config channel does not match service channel {}",
                    state.channel
                ),
            }
            .into());
        }

        state.channel = config.channel;
        state.aggregation_period = config.aggregation_period;
        state.depth_limit = config.depth_limit;
        state.data_format = config.data_format;
        state.order_fields = config.order_fields;
        Ok(())
    }

    /// Handle incoming DOM snapshot message
    ///
    /// # Arguments
    ///
    /// * `snapshot` - Snapshot message containing current market state
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure of the snapshot update
    pub async fn handle_snapshot(&self, snapshot: DomSnapshotMessage) -> Result<()> {
        if snapshot.channel == 0 {
            return Err(ChannelError::InvalidChannel {
                channel: snapshot.channel,
                reason: "snapshot channel must be greater than zero".to_string(),
            }
            .into());
        }

        let mut state = self.state.write().await;

        if state.channel != 0 && state.channel != snapshot.channel {
            return Err(ChannelError::InvalidChannel {
                channel: snapshot.channel,
                reason: format!(
                    "snapshot channel does not match service channel {}",
                    state.channel
                ),
            }
            .into());
        }

        let mut bids = Self::validate_entries(snapshot.bids, "bid")?;
        bids.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());

        let mut asks = Self::validate_entries(snapshot.asks, "ask")?;
        asks.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());

        let depth = usize::try_from(state.depth_limit).unwrap_or(usize::MAX);
        if bids.len() > depth {
            bids.truncate(depth);
        }
        if asks.len() > depth {
            asks.truncate(depth);
        }

        state.channel = snapshot.channel;
        state.bids = bids;
        state.asks = asks;
        state.last_update_time = snapshot.time;
        state.sequence = snapshot.sequence;

        Ok(())
    }

    /// Get the current DOM state
    ///
    /// # Returns
    ///
    /// A clone of the current DOM state
    pub async fn get_state(&self) -> DomState {
        self.state.read().await.clone()
    }

    fn validate_entries(entries: Vec<BidAskEntry>, side: &str) -> Result<Vec<BidAskEntry>> {
        for (idx, entry) in entries.iter().enumerate() {
            if !entry.price.is_finite() {
                return Err(ChannelError::InvalidPayload(format!(
                    "{side} entry #{idx} has non-finite price: {}",
                    entry.price
                ))
                .into());
            }
            if !entry.size.is_finite() {
                return Err(ChannelError::InvalidPayload(format!(
                    "{side} entry #{idx} has non-finite size: {}",
                    entry.size
                ))
                .into());
            }
            if entry.size < 0.0 {
                return Err(ChannelError::InvalidPayload(format!(
                    "{side} entry #{idx} has negative size: {}",
                    entry.size
                ))
                .into());
            }
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::errors::DxLinkErrorType;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_dom_service_initialization() {
        let (tx, mut rx) = mpsc::channel(100);
        let service = DomService::new(tx);

        // Initialize with default config
        service.initialize(1, None).await.unwrap();

        // Verify setup message was sent
        let msg = rx.recv().await.unwrap();
        let setup_msg = msg.as_any().downcast_ref::<DomSetupMessage>().unwrap();
        assert_eq!(setup_msg.channel, 1);
        assert_eq!(setup_msg.accept_aggregation_period, Some(1000));
        assert_eq!(setup_msg.accept_depth_limit, Some(10));
        assert_eq!(setup_msg.accept_data_format, Some("FULL".to_string()));
        assert_eq!(
            setup_msg.accept_order_fields.as_ref().unwrap(),
            &vec!["price".to_string(), "size".to_string()]
        );

        let state = service.get_state().await;
        assert_eq!(state.channel, 1);
        assert_eq!(state.aggregation_period, 1000);
        assert_eq!(state.depth_limit, 10);
        assert_eq!(state.last_update_time, 0);
        assert!(state.sequence.is_none());
        assert!(state.bids.is_empty());
        assert!(state.asks.is_empty());
    }

    #[tokio::test]
    async fn test_dom_service_config_handling() {
        let (tx, _rx) = mpsc::channel(100);
        let service = DomService::new(tx);

        let config = DomConfigMessage {
            message_type: "DOM_CONFIG".to_string(),
            channel: 1,
            aggregation_period: 500,
            depth_limit: 20,
            data_format: "FULL".to_string(),
            order_fields: vec!["price".to_string(), "size".to_string()],
        };

        service.handle_config(config).await.unwrap();
        let state = service.get_state().await;
        assert_eq!(state.aggregation_period, 500);
        assert_eq!(state.depth_limit, 20);
        assert_eq!(state.channel, 1);
    }

    #[tokio::test]
    async fn test_dom_service_snapshot_handling() {
        let (tx, _rx) = mpsc::channel(100);
        let service = DomService::new(tx);

        service.initialize(1, None).await.unwrap();

        let snapshot = DomSnapshotMessage {
            message_type: "DOM_SNAPSHOT".to_string(),
            channel: 1,
            time: 1234567890,
            sequence: Some(42),
            bids: vec![BidAskEntry {
                price: 100.0,
                size: 10.0,
            }],
            asks: vec![BidAskEntry {
                price: 101.0,
                size: 5.0,
            }],
        };

        service.handle_snapshot(snapshot).await.unwrap();
        let state = service.get_state().await;
        assert_eq!(state.bids.len(), 1);
        assert_eq!(state.asks.len(), 1);
        assert_eq!(state.bids[0].price, 100.0);
        assert_eq!(state.asks[0].price, 101.0);
        assert_eq!(state.last_update_time, 1234567890);
        assert_eq!(state.sequence, Some(42));
    }

    #[tokio::test]
    async fn test_dom_service_initialize_rejects_zero_channel() {
        let (tx, _rx) = mpsc::channel(1);
        let service = DomService::new(tx);

        let err = service.initialize(0, None).await.unwrap_err();
        assert_eq!(err.error_type, DxLinkErrorType::BadAction);
    }

    #[tokio::test]
    async fn test_dom_service_initialize_rejects_mismatched_setup_channel() {
        let (tx, _rx) = mpsc::channel(1);
        let service = DomService::new(tx);

        let custom = DomSetupMessage {
            message_type: "DOM_SETUP".to_string(),
            channel: 99,
            accept_aggregation_period: Some(2000),
            accept_depth_limit: Some(5),
            accept_data_format: Some("FULL".to_string()),
            accept_order_fields: Some(vec!["price".to_string(), "size".to_string()]),
        };

        let err = service.initialize(1, Some(custom)).await.unwrap_err();
        assert_eq!(err.error_type, DxLinkErrorType::BadAction);
    }

    #[tokio::test]
    async fn test_dom_service_initialize_propagates_send_error() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let service = DomService::new(tx);

        let err = service.initialize(1, None).await.unwrap_err();
        assert_eq!(err.error_type, DxLinkErrorType::Unknown);
    }

    #[tokio::test]
    async fn test_dom_service_config_channel_mismatch() {
        let (tx, _rx) = mpsc::channel(1);
        let service = DomService::new(tx);
        service.initialize(1, None).await.unwrap();

        let config = DomConfigMessage {
            message_type: "DOM_CONFIG".to_string(),
            channel: 2,
            aggregation_period: 100,
            depth_limit: 5,
            data_format: "FULL".to_string(),
            order_fields: vec!["price".to_string()],
        };

        let err = service.handle_config(config).await.unwrap_err();
        assert_eq!(err.error_type, DxLinkErrorType::BadAction);
    }

    #[tokio::test]
    async fn test_dom_service_snapshot_enforces_depth_limit_and_sorting() {
        let (tx, _rx) = mpsc::channel(1);
        let service = DomService::new(tx);
        service.initialize(1, None).await.unwrap();

        let config = DomConfigMessage {
            message_type: "DOM_CONFIG".to_string(),
            channel: 1,
            aggregation_period: 100,
            depth_limit: 2,
            data_format: "FULL".to_string(),
            order_fields: vec!["price".to_string(), "size".to_string()],
        };
        service.handle_config(config).await.unwrap();

        let snapshot = DomSnapshotMessage {
            message_type: "DOM_SNAPSHOT".to_string(),
            channel: 1,
            time: 77,
            sequence: None,
            bids: vec![
                BidAskEntry {
                    price: 101.0,
                    size: 5.0,
                },
                BidAskEntry {
                    price: 103.0,
                    size: 1.0,
                },
                BidAskEntry {
                    price: 102.0,
                    size: 2.0,
                },
            ],
            asks: vec![
                BidAskEntry {
                    price: 100.5,
                    size: 3.0,
                },
                BidAskEntry {
                    price: 100.2,
                    size: 4.0,
                },
                BidAskEntry {
                    price: 100.1,
                    size: 1.0,
                },
            ],
        };

        service.handle_snapshot(snapshot).await.unwrap();
        let state = service.get_state().await;
        assert_eq!(state.bids.len(), 2);
        assert_eq!(state.bids[0].price, 103.0);
        assert_eq!(state.bids[1].price, 102.0);
        assert_eq!(state.asks.len(), 2);
        assert_eq!(state.asks[0].price, 100.1);
        assert_eq!(state.asks[1].price, 100.2);
        assert_eq!(state.last_update_time, 77);
    }

    #[tokio::test]
    async fn test_dom_service_snapshot_rejects_invalid_entry() {
        let (tx, _rx) = mpsc::channel(1);
        let service = DomService::new(tx);
        service.initialize(1, None).await.unwrap();

        let snapshot = DomSnapshotMessage {
            message_type: "DOM_SNAPSHOT".to_string(),
            channel: 1,
            time: 1,
            sequence: None,
            bids: vec![BidAskEntry {
                price: f64::NAN,
                size: 1.0,
            }],
            asks: vec![BidAskEntry {
                price: 100.0,
                size: 1.0,
            }],
        };

        let err = service.handle_snapshot(snapshot).await.unwrap_err();
        assert_eq!(err.error_type, DxLinkErrorType::InvalidMessage);
    }

    #[tokio::test]
    async fn test_dom_service_snapshot_channel_mismatch() {
        let (tx, _rx) = mpsc::channel(1);
        let service = DomService::new(tx);
        service.initialize(1, None).await.unwrap();

        let snapshot = DomSnapshotMessage {
            message_type: "DOM_SNAPSHOT".to_string(),
            channel: 2,
            time: 1,
            sequence: None,
            bids: vec![BidAskEntry {
                price: 100.0,
                size: 1.0,
            }],
            asks: vec![BidAskEntry {
                price: 101.0,
                size: 2.0,
            }],
        };

        let err = service.handle_snapshot(snapshot).await.unwrap_err();
        assert_eq!(err.error_type, DxLinkErrorType::BadAction);
    }
}
