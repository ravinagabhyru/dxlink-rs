use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::core::{
    auth::{AuthStateChangeListener, DxLinkAuthState},
    channel::DxLinkChannelState,
    client::{ConnectionStateChangeListener, DxLinkConnectionDetails, DxLinkConnectionState},
    errors::{DxLinkError, DxLinkErrorType, ErrorListener},
};

use crate::websocket_client::{
    channel::DxLinkWebSocketChannel,
    config::DxLinkWebSocketClientConfig,
    connector::WebSocketConnector,
    messages::{
        AuthMessage, ChannelRequestMessage, ErrorMessage, KeepaliveMessage, Message, MessageType,
        SetupMessage,
    },
};

const DXLINK_WS_PROTOCOL_VERSION: &str = "0.1";
// const VERSION: &str = env!("CARGO_PKG_VERSION");
const CLIENT_VERSION: &str = concat!("DXF-RS/", env!("CARGO_PKG_VERSION"));

/// WebSocket client for dxLink protocol
pub struct DxLinkWebSocketClient {
    config: DxLinkWebSocketClientConfig,
    connector: Arc<Mutex<Option<WebSocketConnector>>>,
    connection_state: Arc<Mutex<DxLinkConnectionState>>,
    auth_state: Arc<Mutex<DxLinkAuthState>>,
    last_auth_token: Arc<Mutex<Option<String>>>,
    channels: Arc<Mutex<HashMap<u64, Arc<DxLinkWebSocketChannel>>>>,
    next_channel_id: Arc<Mutex<u64>>,
    reconnect_attempts: Arc<Mutex<u32>>,
    last_received: Arc<Mutex<u64>>,
    last_sent: Arc<Mutex<u64>>,
    connection_state_listeners: Arc<Mutex<HashMap<Uuid, ConnectionStateChangeListener>>>,
    auth_state_listeners: Arc<Mutex<HashMap<Uuid, AuthStateChangeListener>>>,
    error_listeners: Arc<Mutex<HashMap<Uuid, ErrorListener>>>,
    connection_details: Arc<Mutex<DxLinkConnectionDetails>>,
    // Track pending channel open requests for synchronization
    pending_channel_opens: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<()>>>>,
    // Track auth timeout for AUTH_STATE responses
    auth_timeout: Arc<Mutex<Option<tokio::time::Instant>>>,
    keepalive_timer: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl DxLinkWebSocketClient {
    pub fn new(config: DxLinkWebSocketClientConfig) -> Self {
        Self {
            config,
            connector: Arc::new(Mutex::new(None)),
            connection_state: Arc::new(Mutex::new(DxLinkConnectionState::NotConnected)),
            auth_state: Arc::new(Mutex::new(DxLinkAuthState::Unauthorized)),
            last_auth_token: Arc::new(Mutex::new(None)),
            channels: Arc::new(Mutex::new(HashMap::new())),
            next_channel_id: Arc::new(Mutex::new(1)),
            reconnect_attempts: Arc::new(Mutex::new(0)),
            last_received: Arc::new(Mutex::new(0)),
            last_sent: Arc::new(Mutex::new(0)),
            connection_state_listeners: Arc::new(Mutex::new(HashMap::new())),
            auth_state_listeners: Arc::new(Mutex::new(HashMap::new())),
            error_listeners: Arc::new(Mutex::new(HashMap::new())),
            connection_details: Arc::new(Mutex::new(DxLinkConnectionDetails {
                protocol_version: DXLINK_WS_PROTOCOL_VERSION.to_string(),
                client_version: CLIENT_VERSION.to_string(),
                server_version: None,
                client_keepalive_timeout: None,
                server_keepalive_timeout: None,
            })),
            pending_channel_opens: Arc::new(Mutex::new(HashMap::new())),
            auth_timeout: Arc::new(Mutex::new(None)),
            keepalive_timer: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn connect(&self, url: String) -> Result<(), Box<dyn std::error::Error>> {
        // Check if already connected to same URL
        if let Some(connector) = self.connector.lock().await.as_ref() {
            if connector.url() == url {
                return Ok(());
            }
        }

        // Disconnect if already connected
        self.disconnect().await?;

        debug!("Connecting to {}", url);
        self.set_connection_state(DxLinkConnectionState::Connecting);

        let connector = WebSocketConnector::new(url);

        // Set up connector callbacks
        let client = self.clone();
        connector
            .set_open_listener(Box::new(move || {
                let client = client.clone();
                tokio::spawn(async move {
                    if let Err(e) = client.process_transport_open().await {
                        error!("Error processing transport open: {}", e);
                    }
                });
            }))
            .await;

        let client = self.clone();
        connector
            .set_message_listener(Box::new(move |msg| {
                let mut client = client.clone();
                tokio::spawn(async move {
                    let t = msg.message_type().to_string();
                    if let Err(e) = client.process_message(msg).await {
                        error!("Error processing message: {}", e);
                    } else {
                        debug!("Message processed successfully {}", t);
                    }
                });
            }))
            .await;

        let client = self.clone();
        connector
            .set_close_listener(Box::new(move |reason, is_error| {
                let client = client.clone();
                tokio::spawn(async move {
                    if let Err(e) = client.process_transport_close(reason, is_error).await {
                        error!("Error processing transport close: {}", e);
                    }
                });
            }))
            .await;

        // Start connector
        connector.start().await?;
        *self.connector.lock().await = Some(connector);

        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), Box<dyn std::error::Error>> {
        if *self.connection_state.lock().await == DxLinkConnectionState::NotConnected {
            return Ok(());
        }

        debug!("Disconnecting");

        // Stop keepalive timer
        self.stop_keepalive_timer().await;

        // Stop connector
        if let Some(connector) = self.connector.lock().await.take() {
            connector.stop().await;
        }

        // Reset state
        *self.last_received.lock().await = 0;
        *self.last_sent.lock().await = 0;
        *self.reconnect_attempts.lock().await = 0;

        self.set_connection_state(DxLinkConnectionState::NotConnected);
        self.set_auth_state(DxLinkAuthState::Unauthorized);

        Ok(())
    }

    pub async fn connection_state(&self) -> DxLinkConnectionState {
        *self.connection_state.lock().await
    }

    pub async fn auth_state(&self) -> DxLinkAuthState {
        *self.auth_state.lock().await
    }

    pub async fn open_channel_internal(
        &mut self,
        service: String,
        parameters: Value,
    ) -> Result<Arc<DxLinkWebSocketChannel>, Box<dyn std::error::Error>> {
        debug!(
            "Client state before open_channel - Connection: {:?}, Auth: {:?}",
            self.connection_state().await,
            self.auth_state().await
        );

        let channel_id = {
            let mut id = self.next_channel_id.lock().await;
            let current = *id;
            *id += 1;
            current
        };

        // Create a oneshot channel for synchronization
        let (open_tx, open_rx) = tokio::sync::oneshot::channel();

        // Store the sender before registering the channel
        {
            let mut pending_opens = self.pending_channel_opens.lock().await;
            pending_opens.insert(channel_id, open_tx);
        }

        let (tx, mut rx) = mpsc::channel::<Box<dyn Message + Send + Sync>>(32);
        let channel = Arc::new(DxLinkWebSocketChannel::new(
            channel_id,
            service.clone(),
            parameters.clone(),
            tx,
            &self.config,
        ));

        // Spawn a task to handle messages from the channel
        let connector = self.connector.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Some(conn) = connector.lock().await.as_ref() {
                    if let Err(e) = conn.send_message(msg).await {
                        error!("Failed to send message from channel {}: {}", channel_id, e);
                    }
                }
            }
        });

        // Add channel to map
        debug!("Adding channel {} to channels map", channel_id);
        self.channels
            .lock()
            .await
            .insert(channel_id, channel.clone());
        debug!(
            "Channels map now contains {} channels",
            self.channels.lock().await.len()
        );

        // Send channel request immediately if connected and authorized
        if *self.connection_state.lock().await == DxLinkConnectionState::Connected
            && *self.auth_state.lock().await == DxLinkAuthState::Authorized
        {
            debug!(
                "Sending channel request for channel {} {} {}",
                channel_id, service, parameters
            );
            if let Err(e) = self
                .send_message(Box::new(MessageType::ChannelRequest(
                    ChannelRequestMessage {
                        message_type: "CHANNEL_REQUEST".to_string(),
                        channel: channel_id,
                        service: service.clone(),
                        parameters: Some(parameters.clone()),
                    },
                )))
                .await
            {
                error!("Failed to send channel request: {}", e);
                return Err(Box::new(DxLinkError::new(
                    DxLinkErrorType::BadAction,
                    format!("Failed to send channel request: {}", e),
                )));
            }

            // Wait for channel to be opened with timeout
            match tokio::time::timeout(Duration::from_secs(5), open_rx).await {
                Ok(Ok(_)) => {
                    debug!("Channel {} opened successfully", channel_id);
                    // Ensure channel is in Opened state
                    channel.process_status_opened();
                }
                Ok(Err(e)) => {
                    error!("Error waiting for channel open confirmation: {}", e);
                    return Err(Box::new(DxLinkError::new(
                        DxLinkErrorType::BadAction,
                        format!("Failed to open channel {}: {}", channel_id, e),
                    )));
                }
                Err(_) => {
                    error!("Timeout waiting for channel open confirmation");
                    return Err(Box::new(DxLinkError::new(
                        DxLinkErrorType::Timeout,
                        format!("Timeout waiting for channel {} to open", channel_id),
                    )));
                }
            }
        }

        Ok(channel)
    }

    pub async fn add_connection_state_listener(
        &self,
        listener: ConnectionStateChangeListener,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.connection_state_listeners
            .lock()
            .await
            .insert(id, listener);
        id
    }

    pub async fn add_auth_state_listener(&self, listener: AuthStateChangeListener) -> Uuid {
        let id = Uuid::new_v4();
        self.auth_state_listeners.lock().await.insert(id, listener);
        id
    }

    pub async fn add_error_listener(&self, listener: ErrorListener) -> Uuid {
        let id = Uuid::new_v4();
        self.error_listeners.lock().await.insert(id, listener);
        id
    }

    pub async fn set_auth_token(&self, token: String) -> Result<(), Box<dyn std::error::Error>> {
        *self.last_auth_token.lock().await = Some(token);
        Ok(())
    }

    pub fn set_connection_state(&self, new_state: DxLinkConnectionState) {
        let connection_state = self.connection_state.clone();
        let connection_state_listeners = self.connection_state_listeners.clone();

        tokio::spawn(async move {
            let mut state = connection_state.lock().await;
            if *state == new_state {
                return;
            }

            let old_state = *state;
            *state = new_state;

            for listener in connection_state_listeners.lock().await.values() {
                listener(&new_state, &old_state);
            }
        });
    }

    pub fn set_auth_state(&self, new_state: DxLinkAuthState) {
        let auth_state = self.auth_state.clone();
        let auth_state_listeners = self.auth_state_listeners.clone();
        tokio::spawn(async move {
            let old_state = *auth_state.lock().await;
            if old_state == new_state {
                return;
            }

            *auth_state.lock().await = new_state;

            for listener in auth_state_listeners.lock().await.values() {
                listener(&new_state, &old_state);
            }
        });
    }

    async fn process_transport_open(&self) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Connection opened");

        // Send SETUP message
        debug!("Sending SETUP message");
        self.send_message(Box::new(MessageType::Setup(SetupMessage {
            message_type: "SETUP".to_string(),
            channel: 0,
            keepalive_timeout: Some(self.config.keepalive_timeout.as_secs() as u64),
            accept_keepalive_timeout: Some(self.config.keepalive_timeout.as_secs() as u64),
            version: format!("{}-{}", DXLINK_WS_PROTOCOL_VERSION, CLIENT_VERSION),
        })))
        .await?;

        Ok(())
    }

    async fn process_transport_close(
        &self,
        reason: String,
        is_error: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Connection closed: {}", reason);

        if is_error {
            self.publish_error(DxLinkError::new(DxLinkErrorType::Unknown, reason.clone()));
        }

        if *self.auth_state.lock().await == DxLinkAuthState::Unauthorized {
            *self.last_auth_token.lock().await = None;
            self.disconnect().await?;
            return Ok(());
        }

        self.reconnect().await?;
        self.set_connection_state(DxLinkConnectionState::NotConnected);
        Ok(())
    }

    pub async fn process_message(
        &mut self,
        message: Box<dyn Message + Send + Sync>,
    ) -> Result<(), DxLinkError> {
        debug!(
            "Client processing message type: {} channel: {}",
            message.message_type(),
            message.channel()
        );

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        *self.last_received.lock().await = now;

        // Process AUTH_STATE messages with higher priority
        if message.message_type() == "AUTH_STATE" && message.channel() == 0 {
            debug!("Processing high-priority AUTH_STATE message");
            if let Err(e) = self.process_auth_state_response(&message).await {
                error!("Error processing AUTH_STATE response: {}", e);
                return Err(DxLinkError::new(
                    DxLinkErrorType::BadAction,
                    format!("Failed to process AUTH_STATE response: {}", e),
                ));
            }
            // Still continue processing normally to handle any additional logic
        }

        let channel_id = message.channel();
        debug!("Checking for channel {} in channels map", channel_id);

        // Handle channel messages first
        if channel_id > 0 {
            debug!("Channel ID is > 0, checking if channel exists in map");
            let channels = self.channels.lock().await;
            if let Some(channel) = channels.get(&channel_id) {
                debug!(
                    "Found channel {} in map, processing message type: {}",
                    channel_id,
                    message.message_type()
                );

                match message.message_type() {
                    "CHANNEL_OPENED" => {
                        debug!("Processing CHANNEL_OPENED for channel {}", channel_id);
                        channel.process_status_opened();
                        channel.process_payload_message(&message);

                        // After processing the message, signal that the channel is open
                        if let Some(tx) =
                            self.pending_channel_opens.lock().await.remove(&channel_id)
                        {
                            debug!("Signaling channel {} is open", channel_id);
                            let _ = tx.send(());
                        }
                    }
                    "ERROR" => {
                        if let Some(error_msg) = message.as_any().downcast_ref::<ErrorMessage>() {
                            debug!(
                                "Received error message for channel {}: {} of type {:?}",
                                channel_id, error_msg.message, error_msg.error
                            );

                            // Create a detailed message for the channel error
                            let detailed_message =
                                format!("Channel {} error: {}", channel_id, error_msg.message);

                            self.publish_error(DxLinkError::new(error_msg.error, detailed_message));
                        }
                    }
                    _ => {
                        channel.process_payload_message(&message);
                    }
                }
                return Ok(());
            } else {
                debug!("Channel {} not found in map", channel_id);
            }
        } else {
            debug!("Channel ID is <= 0, processing as connection message");
        }

        // Process connection messages
        debug!("Processing connection message: {}", message.message_type());
        match message.message_type() {
            "SETUP" => {
                debug!("Processing SETUP response");
                if let Err(e) = self.process_setup_response(&message).await {
                    return Err(DxLinkError::new(
                        DxLinkErrorType::BadAction,
                        format!("Failed to process setup response: {}", e),
                    ));
                }
            }
            "AUTH_STATE" => {
                debug!("Processing AUTH_STATE response");
                if let Err(e) = self.process_auth_state_response(&message).await {
                    return Err(DxLinkError::new(
                        DxLinkErrorType::BadAction,
                        format!("Failed to process auth state response: {}", e),
                    ));
                }
            }
            "ERROR" => {
                if let Some(error_msg) = message.as_any().downcast_ref::<ErrorMessage>() {
                    debug!(
                        "Received error message: {} of type {:?}",
                        error_msg.message, error_msg.error
                    );

                    // Create a more detailed error message that includes the channel information
                    let detailed_message = if error_msg.channel == 0 {
                        // Connection-level error
                        format!("Connection error: {}", error_msg.message)
                    } else {
                        // Channel-specific error
                        format!("Channel {} error: {}", error_msg.channel, error_msg.message)
                    };

                    self.publish_error(DxLinkError::new(error_msg.error, detailed_message));
                }
            }
            "KEEPALIVE" => debug!("Received keepalive message"),
            _ => debug!("Ignoring unknown message type: {}", message.message_type()),
        }

        Ok(())
    }

    async fn process_setup_response(
        &mut self,
        message: &Box<dyn Message + Send + Sync>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if message.message_type() == "SETUP" {
            let payload = message.payload();

            // Validate required fields
            if !payload["version"].is_string() {
                return Err(Box::new(DxLinkError::new(
                    DxLinkErrorType::InvalidMessage,
                    "Missing or invalid version in SETUP response",
                )));
            }

            // Update connection details
            {
                let mut details = self.connection_details.lock().await;
                details.server_version =
                    Some(payload["version"].as_str().unwrap_or("").to_string());

                // Store and use server's keepaliveTimeout
                if let Some(timeout) = payload["keepaliveTimeout"].as_u64() {
                    details.server_keepalive_timeout = Some(timeout as u32);
                    // Update our keepalive timeout to match server's
                    self.config.keepalive_timeout = Duration::from_secs(timeout);
                }
            }

            *self.reconnect_attempts.lock().await = 0;
            self.set_connection_state(DxLinkConnectionState::Connected);

            // Start keepalive timer after successful setup
            debug!(
                "Starting keepalive timer with interval {:?}",
                self.config.keepalive_interval
            );
            self.start_keepalive_timer().await;
        }

        Ok(())
    }

    async fn process_auth_state_response(
        &self,
        message: &Box<dyn Message + Send + Sync>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match message.message_type() {
            "AUTH_STATE" => {
                let payload = message.payload();
                let state = payload["state"].as_str().unwrap_or("");
                // Clear any pending auth timeout since we received a response
                *self.auth_timeout.lock().await = None;

                match state {
                    "AUTHORIZED" => {
                        debug!("Received AUTHORIZED state");
                        self.set_auth_state(DxLinkAuthState::Authorized);

                        // Notify any pending channel requests if we're now authorized
                        if *self.connection_state.lock().await == DxLinkConnectionState::Connected {
                            for (channel_id, channel) in self.channels.lock().await.iter() {
                                if channel.state() == DxLinkChannelState::Requested {
                                    debug!("Sending channel request for previously requested channel {}", channel_id);
                                    if let Err(e) = self
                                        .send_message(Box::new(MessageType::ChannelRequest(
                                            ChannelRequestMessage {
                                                message_type: "CHANNEL_REQUEST".to_string(),
                                                channel: *channel_id,
                                                service: channel.service.clone(),
                                                parameters: Some(channel.parameters.clone()),
                                            },
                                        )))
                                        .await
                                    {
                                        error!("Failed to send channel request: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    "UNAUTHORIZED" => {
                        debug!("Received UNAUTHORIZED state");
                        self.set_auth_state(DxLinkAuthState::Unauthorized);
                        // Clear auth token on unauthorized state
                        *self.last_auth_token.lock().await = None;
                    }
                    _ => {
                        warn!("Unknown auth state: {}", state);
                        let error = DxLinkError::new(
                            DxLinkErrorType::BadAction,
                            format!("Unknown auth state: {}", state),
                        );
                        self.publish_error(error.clone());
                        return Err(Box::new(error));
                    }
                }
            }
            _ => {
                warn!("Unexpected message type in process_auth_state_response");
                let error = DxLinkError::new(
                    DxLinkErrorType::BadAction,
                    "Unexpected message type in auth state response".to_string(),
                );
                self.publish_error(error.clone());
                return Err(Box::new(error));
            }
        }
        Ok(())
    }

    async fn reconnect(&self) -> Result<(), Box<dyn std::error::Error>> {
        {
            let attempts = *self.reconnect_attempts.lock().await;
            if self.config.max_reconnect_attempts > 0
                && attempts >= self.config.max_reconnect_attempts
            {
                warn!("Max reconnect attempts reached");
                self.disconnect().await?;
                return Ok(());
            }
        }

        if *self.connection_state.lock().await == DxLinkConnectionState::NotConnected {
            return Ok(());
        }

        debug!("Attempting to reconnect");

        if let Some(connector) = self.connector.lock().await.take() {
            connector.stop().await;
        }

        *self.last_received.lock().await = 0;
        *self.last_sent.lock().await = 0;

        {
            let mut attempts = self.reconnect_attempts.lock().await;
            *attempts += 1;
        }

        self.set_connection_state(DxLinkConnectionState::Connecting);

        for channel in self.channels.lock().await.values() {
            if channel.state() != DxLinkChannelState::Closed {
                channel.process_status_requested();
            }
        }

        Ok(())
    }

    async fn send_message(
        &self,
        message: Box<dyn Message + Send + Sync>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Sending message: {:?}", message.message_type());

        if let Some(connector) = self.connector.lock().await.as_ref() {
            connector.send_message(message).await?;

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            *self.last_sent.lock().await = now;
        }

        Ok(())
    }

    pub async fn send_auth_message(&self, token: String) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Sending auth message");

        // Validate token
        if token.is_empty() {
            return Err(Box::new(DxLinkError::new(
                DxLinkErrorType::BadAction,
                "Auth token cannot be empty",
            )));
        }

        // Set auth state to Authorizing
        self.set_auth_state(DxLinkAuthState::Authorizing);

        // Send auth message
        let auth_msg = Box::new(MessageType::Auth(AuthMessage {
            message_type: "AUTH".to_string(),
            channel: 0,
            token,
        }));

        // Set up auth timeout - will be checked in the message processing loop
        *self.auth_timeout.lock().await =
            Some(tokio::time::Instant::now() + Duration::from_secs(5));

        // Spawn a timeout handler
        let client = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;

            // Check if still waiting for auth response
            let auth_state = *client.auth_state.lock().await;
            let mut auth_timeout_lock = client.auth_timeout.lock().await;

            if auth_state == DxLinkAuthState::Authorizing && auth_timeout_lock.is_some() {
                // Clear the timeout as we're handling it now
                *auth_timeout_lock = None;

                // Auth timed out, set to unauthorized
                client.set_auth_state(DxLinkAuthState::Unauthorized);

                // Publish timeout error
                client.publish_error(DxLinkError::new(
                    DxLinkErrorType::Timeout,
                    "Timeout waiting for authentication response".to_string(),
                ));
            }
        });

        self.send_message(auth_msg).await?;

        // AUTH_STATE response will be processed by process_auth_state_response
        // when it arrives through the normal message processing flow

        Ok(())
    }

    pub fn publish_error(&self, error: DxLinkError) {
        debug!(
            "Publishing error: {:?} - {}",
            error.error_type, error.message
        );
        let error_listeners = self.error_listeners.clone();
        tokio::spawn(async move {
            let listeners = error_listeners.lock().await;
            if listeners.is_empty() {
                error!(
                    "Unhandled dxLink error: {:?} - {}",
                    error.error_type, error.message
                );
                return;
            }

            for listener in listeners.values() {
                listener(&error);
            }
        });
    }

    // Making this method public to avoid unused warning
    pub async fn get_connection_state(&self) -> DxLinkConnectionState {
        *self.connection_state.lock().await
    }

    // Making this method public to avoid unused warning
    pub async fn get_auth_state(&self) -> DxLinkAuthState {
        *self.auth_state.lock().await
    }

    pub async fn add_connection_state_change_listener(
        &mut self,
        listener: ConnectionStateChangeListener,
    ) {
        let id = Uuid::new_v4();
        self.connection_state_listeners
            .lock()
            .await
            .insert(id, listener);
    }

    pub async fn add_auth_state_change_listener(&mut self, listener: AuthStateChangeListener) {
        let id = Uuid::new_v4();
        self.auth_state_listeners.lock().await.insert(id, listener);
    }

    pub async fn get_connection_details(&self) -> &DxLinkConnectionDetails {
        let details = self.connection_details.lock().await;
        // TODO: this is not ideal, fix it later
        Box::leak(Box::new(details.clone()))
    }

    pub async fn remove_connection_state_change_listener(
        &mut self,
        _listener: ConnectionStateChangeListener,
    ) {
        // TODO: Implement removal by comparing function pointers or using an ID system
    }

    // Making this method public to avoid unused warning
    pub async fn remove_auth_state_change_listener(&mut self, _listener: AuthStateChangeListener) {
        // TODO: Implement removal by comparing function pointers or using an ID system
    }

    // Making this method public to avoid unused warning
    pub async fn remove_error_listener(&mut self, _listener: ErrorListener) {
        // TODO: Implement removal by comparing function pointers or using an ID system
    }

    // async fn open_channel(
    //     &mut self,
    //     service: String,
    //     parameters: HashMap<String, Value>,
    // ) -> Box<dyn DxLinkChannel + Send + Sync> {
    //     match DxLinkWebSocketClient::open_channel(
    //         self,
    //         service,
    //         serde_json::to_value(parameters).unwrap(),
    //     )
    //     .await
    //     {
    //         Ok(channel) => Box::new(channel),
    //         Err(e) => {
    //             error!("Failed to open channel: {}", e);
    //             panic!("Failed to open channel: {}", e); // TODO: Better error handling
    //         }
    //     }
    // }

    pub async fn open_channel(
        &mut self,
        service: String,
        parameters: serde_json::Value,
    ) -> Arc<DxLinkWebSocketChannel> {
        debug!(
            "Opening channel in DxLinkClient implementation with service: {}",
            service
        );
        match self.open_channel_internal(service, parameters).await {
            Ok(channel) => channel,
            Err(e) => {
                error!("Failed to open channel: {}", e);
                panic!("Failed to open channel: {}", e); // TODO: Better error handling
            }
        }
    }

    async fn start_keepalive_timer(&self) {
        // Stop existing timer if any
        self.stop_keepalive_timer().await;

        let interval = self.config.keepalive_interval;
        let client = self.clone();

        let timer_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            loop {
                interval.tick().await;

                // Only send keepalive if connected
                if *client.connection_state.lock().await == DxLinkConnectionState::Connected {
                    debug!("Sending KEEPALIVE message");
                    let keepalive_msg = MessageType::KeepAlive(KeepaliveMessage {
                        message_type: "KEEPALIVE".to_string(),
                        channel: 0,
                    });
                    if let Err(e) = client.send_message(Box::new(keepalive_msg)).await {
                        error!("Failed to send keepalive message: {}", e);
                    }
                }
            }
        });

        *self.keepalive_timer.lock().await = Some(timer_handle);
    }

    async fn stop_keepalive_timer(&self) {
        if let Some(timer) = self.keepalive_timer.lock().await.take() {
            debug!("Stopping keepalive timer");
            timer.abort();
        }
    }
}

impl Clone for DxLinkWebSocketClient {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            connector: self.connector.clone(),
            connection_state: self.connection_state.clone(),
            auth_state: self.auth_state.clone(),
            last_auth_token: self.last_auth_token.clone(),
            channels: self.channels.clone(),
            next_channel_id: self.next_channel_id.clone(),
            reconnect_attempts: self.reconnect_attempts.clone(),
            last_received: self.last_received.clone(),
            last_sent: self.last_sent.clone(),
            connection_state_listeners: self.connection_state_listeners.clone(),
            auth_state_listeners: self.auth_state_listeners.clone(),
            error_listeners: self.error_listeners.clone(),
            connection_details: self.connection_details.clone(),
            pending_channel_opens: self.pending_channel_opens.clone(),
            auth_timeout: self.auth_timeout.clone(),
            keepalive_timer: self.keepalive_timer.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_lifecycle() {
        let config = DxLinkWebSocketClientConfig::default();
        let client = DxLinkWebSocketClient::new(config);

        assert_eq!(
            client.connection_state().await,
            DxLinkConnectionState::NotConnected
        );
        assert_eq!(client.auth_state().await, DxLinkAuthState::Unauthorized);

        // Test connection state changes
        let state_changes = Arc::new(Mutex::new(Vec::new()));
        let state_changes_clone = state_changes.clone();

        client
            .add_connection_state_listener(Box::new(move |new_state, old_state| {
                let state_changes = state_changes_clone.clone();
                let new_state_clone = new_state.clone();
                let old_state_clone = old_state.clone();
                tokio::spawn(async move {
                    state_changes
                        .lock()
                        .await
                        .push((new_state_clone, old_state_clone));
                });
            }))
            .await;

        client
            .connect("wss://demo.dxfeed.com/dxlink-ws".to_string())
            .await
            .unwrap();

        let changes = state_changes.lock().await;
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0],
            (
                DxLinkConnectionState::Connecting,
                DxLinkConnectionState::NotConnected
            )
        );
    }

    #[tokio::test]
    async fn test_auth_flow() {
        let config = DxLinkWebSocketClientConfig::default();
        let client = DxLinkWebSocketClient::new(config);

        let auth_changes = Arc::new(Mutex::new(Vec::new()));
        let auth_changes_clone = auth_changes.clone();

        client
            .add_auth_state_listener(Box::new(move |new_state, old_state| {
                let auth_changes = auth_changes_clone.clone();
                let new_state_clone = new_state.clone();
                let old_state_clone = old_state.clone();
                tokio::spawn(async move {
                    auth_changes
                        .lock()
                        .await
                        .push((new_state_clone, old_state_clone));
                });
            }))
            .await;

        client
            .set_auth_token("test_token".to_string())
            .await
            .unwrap();

        assert!(client.last_auth_token.lock().await.is_some());
        assert_eq!(
            client.last_auth_token.lock().await.as_ref().unwrap(),
            "test_token"
        );
    }
}
