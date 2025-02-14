//! Core traits and types for DXLink clients.
//!
//! This module provides the foundational types and interfaces for implementing DXLink clients,
//! including connection state management, authentication handling, and channel creation.
//!
//! The main trait [`DxLinkClient`] defines the core functionality that all DXLink client
//! implementations must provide. It handles:
//!
//! - Connection lifecycle (connect, disconnect, reconnect)
//! - Authentication and authorization
//! - Channel management
//! - Event listening for state changes and errors
//!
//! Key types include:
//! - [`DxLinkConnectionState`] - Represents the current connection state
//! - [`DxLinkConnectionDetails`] - Contains connection metadata and configuration
//! - Various listener types for handling events and state changes
use std::fmt;
use crate::core::{
    auth::{DxLinkAuthState, AuthStateChangeListener},
    channel::DxLinkChannel,
    errors::ErrorListener,
};

/// Connection state that can be used to check if connection is established and ready to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DxLinkConnectionState {
    /// Client was created and not connected to remote endpoint.
    NotConnected,
    /// The connect method was called to establish connection or reconnect is in progress.
    /// The connection is not ready to use yet.
    Connecting,
    /// The connection to remote endpoint is established.
    /// The connection is ready to use.
    Connected,
}

impl fmt::Display for DxLinkConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DxLinkConnectionState::NotConnected => write!(f, "NOT_CONNECTED"),
            DxLinkConnectionState::Connecting => write!(f, "CONNECTING"),
            DxLinkConnectionState::Connected => write!(f, "CONNECTED"),
        }
    }
}

/// Connection details that can be used for debugging or logging.
#[derive(Debug, Clone)]
pub struct DxLinkConnectionDetails {
    /// Protocol version used for connection to the remote endpoint.
    pub protocol_version: String,
    /// Version of the client library.
    pub client_version: String,
    /// Version of the server which the client is connected to.
    pub server_version: Option<String>,
    /// Timeout in seconds for server to detect that client is disconnected.
    pub client_keepalive_timeout: Option<u32>,
    /// Timeout in seconds for client to detect that server is disconnected.
    pub server_keepalive_timeout: Option<u32>,
}

/// Callback type for connection state changes.
pub type ConnectionStateChangeListener = Box<dyn Fn(&DxLinkConnectionState, &DxLinkConnectionState) + Send + Sync>;

/// dxLink client that can be used to connect to the remote dxLink endpoint and open channels to services.
#[async_trait::async_trait]
pub trait DxLinkClient: Send + Sync {
    /// Connect to the remote endpoint.
    /// Connects to the specified remote address. Previously established connections are closed if the new address is different from the old one.
    /// This method does nothing if address does not change.
    ///
    /// For connection with the authorization token, use `set_auth_token` before calling this method.
    /// If the token is not set, the connection will be established without authorization.
    async fn connect(&mut self, url: String);

    /// Reconnect to the remote endpoint.
    /// This method does nothing if the client is not connected.
    async fn reconnect(&mut self);

    /// Disconnect from the remote endpoint.
    /// This method does nothing if the client is not connected.
    async fn disconnect(&mut self);

    /// Get connection details that can be used for debugging or logging.
    async fn get_connection_details(&self) -> &DxLinkConnectionDetails;

    /// Get connection state that can be used to check if connection is established and ready to use.
    async fn get_connection_state(&self) -> DxLinkConnectionState;

    /// Add a listener for connection state changes.
    async fn add_connection_state_change_listener(&mut self, listener: ConnectionStateChangeListener);

    /// Remove a listener for connection state changes.
    async fn remove_connection_state_change_listener(&mut self, listener: ConnectionStateChangeListener);

    /// Set authorization token to be used for connection to the remote endpoint.
    /// This method does nothing if the client is connected.
    async fn set_auth_token(&mut self, token: String);

    /// Get authentication state that can be used to check if user is authorized on the remote endpoint.
    async fn get_auth_state(&self) -> DxLinkAuthState;

    /// Add a listener for authentication state changes.
    async fn add_auth_state_change_listener(&mut self, listener: AuthStateChangeListener);

    /// Remove a listener for authentication state changes.
    async fn remove_auth_state_change_listener(&mut self, listener: AuthStateChangeListener);

    /// Add a listener for errors from the server.
    async fn add_error_listener(&mut self, listener: ErrorListener);

    /// Remove a listener for errors from the server.
    async fn remove_error_listener(&mut self, listener: ErrorListener);

    /// Open an isolated channel to service within single connection to remote endpoint.
    async fn open_channel(
        &mut self,
        service: String,
        parameters: serde_json::Value,
    ) -> Box<dyn DxLinkChannel + Send + Sync>;

    /// Close the client and free all resources.
    async fn close(&mut self);
}
