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

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use crate::core::{Result, errors::ChannelError};
use crate::dom::messages::{BidAskEntry, DomSetupMessage, DomConfigMessage, DomSnapshotMessage, Message};

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
        let mut state = self.state.write().await;
        state.channel = channel;

        // Send setup message with configuration
        let setup_msg = if let Some(config) = config {
            config
        } else {
            DomSetupMessage {
                message_type: "DOM_SETUP".to_string(),
                channel,
                accept_aggregation_period: Some(state.aggregation_period),
                accept_depth_limit: Some(state.depth_limit),
                accept_data_format: Some(state.data_format.clone()),
                accept_order_fields: Some(state.order_fields.clone()),
            }
        };

        self.tx.send(Box::new(setup_msg)).await
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
        let mut state = self.state.write().await;
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
        let mut state = self.state.write().await;
        
        // Validate and sort bids (highest price first)
        let mut bids = snapshot.bids;
        bids.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap_or(std::cmp::Ordering::Equal));
        
        // Validate and sort asks (lowest price first)
        let mut asks = snapshot.asks;
        asks.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap_or(std::cmp::Ordering::Equal));
        
        // Update state
        state.bids = bids;
        state.asks = asks;
        
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
}

#[cfg(test)]
mod tests {
    use super::*;
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
    }

    #[tokio::test]
    async fn test_dom_service_snapshot_handling() {
        let (tx, _rx) = mpsc::channel(100);
        let service = DomService::new(tx);

        let snapshot = DomSnapshotMessage {
            message_type: "DOM_SNAPSHOT".to_string(),
            channel: 1,
            time: 1234567890,
            bids: vec![BidAskEntry { price: 100.0, size: 10.0 }],
            asks: vec![BidAskEntry { price: 101.0, size: 5.0 }],
        };

        service.handle_snapshot(snapshot).await.unwrap();
        let state = service.get_state().await;
        assert_eq!(state.bids.len(), 1);
        assert_eq!(state.asks.len(), 1);
        assert_eq!(state.bids[0].price, 100.0);
        assert_eq!(state.asks[0].price, 101.0);
    }
} 