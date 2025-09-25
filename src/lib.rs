//! dxlink-rs: A high-performance Rust client library for dxFeed/dxLink Market Data
//!
//! This library provides functionality for connecting to dxFeed/dxLink Market Data,
//! enabling real-time market data streaming, subscription management, and more.

#![allow(missing_docs)]

use tracing;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

pub mod core;
pub mod dom;
pub mod feed;
pub mod websocket_client;

#[cfg(feature = "wasm")]
mod wasm;

/// Initialize the library with default configuration
///
/// This function must be called before using any other functionality,
/// especially in WASM environments.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn init() {
    // Initialize tracing subscriber
    tracing_subscriber::fmt::init();
    tracing::info!("dxlink-rs initialized");
}

// Re-export main types for convenient usage
pub use crate::core::channel::{
    ChannelErrorListener, ChannelMessageListener, ChannelStateChangeListener, DxLinkChannelMessage,
    DxLinkChannelState,
};
pub use crate::core::client::{
    ConnectionStateChangeListener, DxLinkConnectionDetails, DxLinkConnectionState,
};
pub use crate::core::errors::{DxLinkError, DxLinkErrorType, Result};
pub use crate::dom::messages::{
    BidAskEntry, DomConfigMessage, DomSetupMessage, DomSnapshotMessage,
};
pub use crate::dom::DomService;
pub use crate::feed::messages::{
    FeedConfigMessage, FeedContract, FeedData, FeedDataMessage, FeedSetupMessage,
    FeedSubscriptionEntry, FeedSubscriptionMessage,
};
pub use crate::feed::{Feed, FeedAcceptConfig, FeedDataFormat, FeedEventFields, FeedOptions};
pub use crate::websocket_client::{
    DxLinkLogLevel, DxLinkWebSocketChannel, DxLinkWebSocketClientConfig,
};
