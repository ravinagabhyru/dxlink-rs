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
    client::{
        ConnectionStateChangeListener, DxLinkConnectionDetails, DxLinkConnectionState,
    },
    errors::{DxLinkError, DxLinkErrorType, ErrorListener},
};

use crate::websocket_client::ChannelRequestMessage;
use crate::websocket_client::{
    channel::DxLinkWebSocketChannel,
    config::DxLinkWebSocketClientConfig,
    connector::WebSocketConnector,
    messages::{
        util::{is_channel_lifecycle_message, is_channel_message, is_connection_message},
        AuthMessage, ErrorMessage, KeepaliveMessage, Message, MessageType, SetupMessage,
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
    channels: HashMap<u64, Arc<DxLinkWebSocketChannel>>,
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
}

impl DxLinkWebSocketClient {
    pub fn new(config: DxLinkWebSocketClientConfig) -> Self {
        Self {
            config,
            connector: Arc::new(Mutex::new(None)),
            connection_state: Arc::new(Mutex::new(DxLinkConnectionState::NotConnected)),
            auth_state: Arc::new(Mutex::new(DxLinkAuthState::Unauthorized)),
            last_auth_token: Arc::new(Mutex::new(None)),
            channels: HashMap::new(),
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
                let client = client.clone();
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
            *id += 2;
            current
        };

        // Create a oneshot channel for synchronization
        let (open_tx, open_rx) = tokio::sync::oneshot::channel();
        
        // Store the sender before registering the channel
        {
            let mut pending_opens = self.pending_channel_opens.lock().await;
            pending_opens.insert(channel_id, open_tx);
        }

        let (tx, _rx) = mpsc::channel::<Box<dyn Message + Send + Sync>>(32);
        let channel = Arc::new(DxLinkWebSocketChannel::new(
            channel_id,
            service.clone(),
            parameters.clone(),
            tx,
            &self.config,
        ));

        self.channels.insert(channel_id, channel.clone());

        // Send channel request immediately if connected and authorized
        if *self.connection_state.lock().await == DxLinkConnectionState::Connected
            && *self.auth_state.lock().await == DxLinkAuthState::Authorized
        {
            debug!("Sending channel request for channel {} {} {}", channel_id, service, parameters);
            self.send_message(Box::new(MessageType::ChannelRequest(
                ChannelRequestMessage {
                    message_type: "CHANNEL_REQUEST".to_string(),
                    channel: channel_id,
                    service: service.clone(),
                    parameters: Some(parameters.clone()),
                },
            )))
            .await?;
            
            // Wait for channel to be opened with timeout
            match tokio::time::timeout(Duration::from_secs(5), open_rx).await {
                Ok(Ok(_)) => {
                    debug!("Channel {} opened successfully", channel_id);
                },
                Ok(Err(e)) => {
                    error!("Error waiting for channel open confirmation: {}", e);
                    return Err(Box::new(DxLinkError::new(
                        DxLinkErrorType::BadAction,
                        format!("Failed to open channel {}: {}", channel_id, e),
                    )));
                },
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

        // Connection state will be set to Connected when we receive SETUP response
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

    async fn process_message(
        &self,
        message: Box<dyn Message + Send + Sync>,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        if is_connection_message(&message) {
            debug!("Processing connection message: {}", message.message_type());
            match message.message_type() {
                "SETUP" => {
                    debug!("Processing SETUP response");
                    self.process_setup_message(&message).await?
                }
                "AUTH" => {
                    debug!("Processing AUTH response");
                }
                "AUTH_STATE" => {
                    debug!("Processing AUTH_STATE response");
                    self.process_auth_state_message(&message).await?;
                }
                "ERROR" => {
                    if let Some(error_msg) = message.as_any().downcast_ref::<ErrorMessage>() {
                        debug!("Received error message: {}", error_msg.message);
                        self.publish_error(DxLinkError::new(
                            DxLinkErrorType::Unknown,
                            error_msg.message.clone(),
                        ));
                    }
                }
                "KEEPALIVE" => debug!("Received keepalive message"), // Ignore keepalive messages
                _ => warn!(
                    "Unknown connection message type: {}",
                    message.message_type()
                ),
            }
        } else if is_channel_message(&message) {
            let channel_id = message.channel();
            if channel_id > 0 {
                if let Some(channel) = self.channels.get(&channel_id) {
                    if message.message_type() == "ERROR" {
                        if let Some(error_msg) = message.as_any().downcast_ref::<ErrorMessage>() {
                            debug!("Received error message: {}", error_msg.message);
                            self.publish_error(DxLinkError::new(
                                DxLinkErrorType::Unknown,
                                error_msg.message.clone(),
                            ));
                        }
                    } else if is_channel_lifecycle_message(&message) {
                        match message.message_type() {
                            "CHANNEL_OPENED" => {
                                debug!(
                                    "Processing CHANNEL_OPENED message for channel {}",
                                    channel_id
                                );
                                channel.process_status_opened();
                                channel.process_payload_message(&message);
                                
                                // Notify any waiting open_channel call
                                if let Some(tx) = self.pending_channel_opens.lock().await.remove(&channel_id) {
                                    debug!(
                                        "Sending channel open confirmation for channel {}",
                                        channel_id
                                    );
                                    let _ = tx.send(()); // Ignore errors as the receiver might be dropped
                                }
                            }
                            "CHANNEL_CLOSED" => {
                                debug!(
                                    "Processing CHANNEL_CLOSED message for channel {}",
                                    channel_id
                                );
                                channel.process_status_closed();
                                channel.process_payload_message(&message);
                            }
                            _ => {}
                        }
                    } else {
                        debug!(
                            "Processing payload message for channel {}: {}",
                            channel_id,
                            message.message_type()
                        );
                        channel.process_payload_message(&message);
                    }
                } else if message.message_type() == "CHANNEL_OPENED" {
                    debug!("Received CHANNEL_OPENED for channel {} that isn't in channel map yet", channel_id);
                    
                    // If we find a pending open request, notify it
                    if let Some(tx) = self.pending_channel_opens.lock().await.remove(&channel_id) {
                        debug!("Sending channel open confirmation for channel {}", channel_id);
                        let _ = tx.send(());
                    }
                } else {
                    warn!("Received message for unknown channel: {}", channel_id);
                }
            }
        }

        if now - *self.last_sent.lock().await
            >= self.config.keepalive_interval.as_secs() as u64 * 1000
        {
            debug!("Sending keepalive message");
            self.send_message(Box::new(MessageType::KeepAlive(KeepaliveMessage {
                message_type: "KEEPALIVE".to_string(),
                channel: 0,
            })))
            .await?;
        }

        Ok(())
    }

    async fn process_setup_message(
        &self,
        message: &Box<dyn Message + Send + Sync>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if message.message_type() == "SETUP" {
            let payload = message.payload();
            // Update connection details
            {
                let mut details = self.connection_details.lock().await;
                details.server_version =
                    Some(payload["version"].as_str().unwrap_or("").to_string());
                details.server_keepalive_timeout =
                    payload["keepaliveTimeout"].as_u64().map(|t| t as u32);
            }

            *self.reconnect_attempts.lock().await = 0;
            self.set_connection_state(DxLinkConnectionState::Connected);
        }

        Ok(())
    }

    async fn process_auth_state_message(
        &self,
        message: &Box<dyn Message + Send + Sync>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match message.message_type() {
            "AUTH_STATE" => {
                let payload = message.payload();
                let state = payload["state"].as_str().unwrap_or("");
                match state {
                    "AUTHORIZED" => self.set_auth_state(DxLinkAuthState::Authorized),
                    "UNAUTHORIZED" => self.set_auth_state(DxLinkAuthState::Unauthorized),
                    _ => warn!("Unknown auth state: {}", state),
                }
            }
            _ => warn!("Unexpected message type in process_auth_state_message"),
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

        for channel in self.channels.values() {
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

    async fn send_auth_message(&self, token: String) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Sending auth message");
        self.set_auth_state(DxLinkAuthState::Authorizing);

        let auth_msg = Box::new(MessageType::Auth(AuthMessage {
            message_type: "AUTH".to_string(),
            channel: 0,
            token,
        }));

        self.send_message(auth_msg).await
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

    async fn get_connection_state(&self) -> DxLinkConnectionState {
        *self.connection_state.lock().await
    }

    async fn get_auth_state(&self) -> DxLinkAuthState {
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

    async fn remove_auth_state_change_listener(&mut self, _listener: AuthStateChangeListener) {
        // TODO: Implement removal by comparing function pointers or using an ID system
    }

    async fn remove_error_listener(&mut self, _listener: ErrorListener) {
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
        debug!("Opening channel in DxLinkClient implementation with service: {}", service);
        match self.open_channel_internal(service, parameters).await {
            Ok(channel) => channel,
            Err(e) => {
                error!("Failed to open channel: {}", e);
                panic!("Failed to open channel: {}", e); // TODO: Better error handling
            }
        }
    }
}

// #[async_trait]
// impl DxLinkClient for DxLinkWebSocketClient {
    // async fn connect(&mut self, url: String) {
    //     if let Err(e) = DxLinkWebSocketClient::connect(self, url).await {
    //         error!("Failed to connect: {}", e);
    //     }
    // }

    // async fn reconnect(&mut self) {
    //     if let Err(e) = DxLinkWebSocketClient::reconnect(self).await {
    //         error!("Failed to reconnect: {}", e);
    //     }
    // }

    // async fn disconnect(&mut self) {
    //     if let Err(e) = DxLinkWebSocketClient::disconnect(self).await {
    //         error!("Failed to disconnect: {}", e);
    //     }
    // }

    // async fn get_connection_state(&self) -> DxLinkConnectionState {
    //     *self.connection_state.lock().await
    // }

    // async fn get_auth_state(&self) -> DxLinkAuthState {
    //     *self.auth_state.lock().await
    // }

    // async fn add_connection_state_change_listener(
    //     &mut self,
    //     listener: ConnectionStateChangeListener,
    // ) {
    //     let id = Uuid::new_v4();
    //     self.connection_state_listeners
    //         .lock()
    //         .await
    //         .insert(id, listener);
    // }

    // async fn add_auth_state_change_listener(&mut self, listener: AuthStateChangeListener) {
    //     let id = Uuid::new_v4();
    //     self.auth_state_listeners.lock().await.insert(id, listener);
    // }

    // async fn add_error_listener(&mut self, listener: ErrorListener) {
    //     let id = Uuid::new_v4();
    //     self.error_listeners.lock().await.insert(id, listener);
    // }

    // async fn get_connection_details(&self) -> &DxLinkConnectionDetails {
    //     let details = self.connection_details.lock().await;
    //     // TODO: this is not ideal, fix it later
    //     Box::leak(Box::new(details.clone()))
    // }

    // async fn remove_connection_state_change_listener(
    //     &mut self,
    //     _listener: ConnectionStateChangeListener,
    // ) {
    //     // TODO: Implement removal by comparing function pointers or using an ID system
    // }

    // async fn set_auth_token(&mut self, token: String) {
    //     let client = self.clone();
    //     tokio::spawn(async move {
    //         if let Err(e) = client.set_auth_token(token).await {
    //             error!("Failed to set auth token: {}", e);
    //         }
    //     });
    // }

    // async fn remove_auth_state_change_listener(&mut self, _listener: AuthStateChangeListener) {
    //     // TODO: Implement removal by comparing function pointers or using an ID system
    // }

    // async fn remove_error_listener(&mut self, _listener: ErrorListener) {
    //     // TODO: Implement removal by comparing function pointers or using an ID system
    // }

    // // async fn open_channel(
    // //     &mut self,
    // //     service: String,
    // //     parameters: HashMap<String, Value>,
    // // ) -> Box<dyn DxLinkChannel + Send + Sync> {
    // //     match DxLinkWebSocketClient::open_channel(
    // //         self,
    // //         service,
    // //         serde_json::to_value(parameters).unwrap(),
    // //     )
    // //     .await
    // //     {
    // //         Ok(channel) => Box::new(channel),
    // //         Err(e) => {
    // //             error!("Failed to open channel: {}", e);
    // //             panic!("Failed to open channel: {}", e); // TODO: Better error handling
    // //         }
    // //     }
    // // }

    // async fn open_channel(
    //     &mut self,
    //     service: String,
    //     parameters: serde_json::Value,
    // ) -> Box<dyn DxLinkChannel + Send + Sync> {
    //     debug!("Opening channel in DxLinkClient implementation with service: {}", service);
    //     match self.open_channel_internal(service, parameters).await {
    //         Ok(channel) => Box::new(channel),
    //         Err(e) => {
    //             error!("Failed to open channel: {}", e);
    //             panic!("Failed to open channel: {}", e); // TODO: Better error handling
    //         }
    //     }
    // }

    // async fn close(&mut self) {
    //     if let Err(e) = DxLinkWebSocketClient::disconnect(self).await {
    //         error!("Failed to close: {}", e);
    //     }
    // }
// }

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
