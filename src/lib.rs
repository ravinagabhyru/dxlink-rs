//! dxlink-rs: A high-performance Rust client library for dxFeed/dxLink Market Data
//!
//! This library provides functionality for connecting to dxFeed/dxLink Market Data,
//! enabling real-time market data streaming, subscription management, and more.

#![allow(missing_docs)]

use tracing;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

pub mod core;
pub mod feed;
pub mod dom;
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
pub use crate::core::errors::{DxLinkError, DxLinkErrorType, Result};
pub use crate::core::client::{
    DxLinkConnectionDetails, DxLinkConnectionState,
    ConnectionStateChangeListener,
};
pub use crate::core::channel::{
    ChannelMessageListener, ChannelStateChangeListener, ChannelErrorListener,
    DxLinkChannelMessage, DxLinkChannelState,
};
pub use crate::feed::{
    Feed, FeedAcceptConfig, FeedDataFormat, FeedEventFields, FeedOptions,
};
pub use crate::feed::messages::
    {FeedSubscriptionEntry, FeedSetupMessage, FeedConfigMessage,
    FeedSubscriptionMessage, FeedDataMessage, FeedData, FeedContract};
pub use crate::dom::messages::{DomSetupMessage, DomConfigMessage, DomSnapshotMessage, BidAskEntry};
pub use crate::dom::DomService;
pub use crate::websocket_client::{
    DxLinkWebSocketChannel, DxLinkWebSocketClientConfig, DxLinkLogLevel
};
