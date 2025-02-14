use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::Sender;
use crate::{ChannelErrorListener, ChannelMessageListener, ChannelStateChangeListener, DxLinkChannelMessage};
use tracing::error;
use uuid::Uuid;

use crate::core::channel;
use crate::core::channel::DxLinkChannel;
use crate::core::channel::DxLinkChannelState;
use crate::websocket_client::{
    config::DxLinkWebSocketClientConfig,
    messages::{ChannelRequestMessage, Message},
};
use crate::DxLinkError;

use super::messages::MessageType;
use super::ChannelCancelMessage;

/// Wrapper for callbacks to enable equality comparison
#[derive(Clone)]
struct CallbackWrapper<T> {
    id: Uuid,
    callback: Arc<T>,
}

impl<T> PartialEq for CallbackWrapper<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for CallbackWrapper<T> {}

impl<T> std::hash::Hash for CallbackWrapper<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// WebSocket channel implementation
pub struct DxLinkWebSocketChannel {
    /// Unique channel identifier
    pub id: u64,
    /// Service name this channel connects to
    pub service: String,
    /// Channel parameters
    pub parameters: Value,
    /// Current channel state
    state: Arc<Mutex<DxLinkChannelState>>,
    /// Message sender
    message_sender: Sender<Box<dyn Message + Send + Sync>>,
    /// Message listeners
    message_listeners: Arc<Mutex<HashSet<CallbackWrapper<ChannelMessageListener>>>>,
    /// State change listeners
    state_listeners: Arc<Mutex<HashSet<CallbackWrapper<ChannelStateChangeListener>>>>,
    /// Error listeners
    error_listeners: Arc<Mutex<HashSet<CallbackWrapper<ChannelErrorListener>>>>,
}

impl DxLinkWebSocketChannel {
    /// Create a new WebSocket channel
    pub fn new(
        id: u64,
        service: String,
        parameters: Value,
        message_sender: Sender<Box<dyn Message + Send + Sync>>,
        _config: &DxLinkWebSocketClientConfig,
    ) -> Self {
        Self {
            id,
            service,
            parameters,
            state: Arc::new(Mutex::new(DxLinkChannelState::Requested)),
            message_sender,
            message_listeners: Arc::new(Mutex::new(HashSet::new())),
            state_listeners: Arc::new(Mutex::new(HashSet::new())),
            error_listeners: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Send a message on this channel
    pub async fn send(
        &self,
        message: DxLinkChannelMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check channel state using direct mutex access
        let channel_state = *self.state.lock().unwrap();
        if channel_state != DxLinkChannelState::Opened {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Channel is not ready",
            )));
        }

        // Send message using message_sender
        // Convert channel message to base Message type
        let msg_payload = message.payload.clone();
        let msg = match message.message_type.as_str() {
            "FEED_SUBSCRIPTION" => MessageType::ChannelRequest(ChannelRequestMessage {
                message_type: message.message_type,
                channel: self.id,
                service: self.service.clone(),
                parameters: Some(msg_payload),
            }),
            _ => {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Unsupported message type: {}", message.message_type),
                )));
            }
        };

        self.message_sender.send(Box::new(msg)).await.map_err(|e| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to send message: {}", e),
            )) as Box<dyn std::error::Error + Send + Sync>
        })?;

        Ok(())
    }

    /// Get current channel state
    pub fn state(&self) -> DxLinkChannelState {
        *self.state.lock().unwrap()
    }

    /// Send an error on this channel
    pub async fn error(
        &self,
        error: DxLinkError,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let error_payload = serde_json::to_value(&error)?;
        self.send(DxLinkChannelMessage {
            message_type: "ERROR".to_string(),
            payload: error_payload,
        })
        .await
    }

    /// Close the channel
    pub async fn close(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.state() == DxLinkChannelState::Closed {
            return Ok(());
        }

        self.set_state(DxLinkChannelState::Closed);
        self.clear();

        // Send channel cancel message
        let msg = Box::new(MessageType::ChannelCancel(ChannelCancelMessage {
            message_type: "CHANNEL_CANCEL".to_string(),
            channel: self.id,
        }));
        self.message_sender.send(msg).await?;

        Ok(())
    }

    /// Request channel open
    pub async fn request(&self) -> Result<(), Box<dyn std::error::Error>> {
        let params: Value = match serde_json::from_value(self.parameters.clone()) {
            Ok(p) => p,
            Err(e) => return Err(format!("Failed to convert parameters: {}", e).into()),
        };

        let msg = Box::new(MessageType::ChannelRequest(ChannelRequestMessage {
            message_type: "CHANNEL_REQUEST".to_string(),
            channel: self.id,
            service: self.service.clone(),
            parameters: Some(params),
        }));
        self.message_sender.send(msg).await?;

        self.process_status_requested();
        Ok(())
    }

    /// Process an incoming payload message
    pub fn process_payload_message(&self, message: &Box<dyn Message + Send + Sync>) {
        let channel_msg = DxLinkChannelMessage {
            message_type: message.message_type().to_string(),
            payload: message.payload().clone(),
        };

        for wrapper in self.message_listeners.lock().unwrap().iter() {
            (wrapper.callback)(&channel_msg);
        }
    }

    /// Process channel opened status
    pub fn process_status_opened(&self) {
        self.set_state(DxLinkChannelState::Opened);
    }

    /// Process channel requested status
    pub fn process_status_requested(&self) {
        self.set_state(DxLinkChannelState::Requested);
    }

    /// Process channel closed status
    pub fn process_status_closed(&self) {
        self.set_state(DxLinkChannelState::Closed);
        self.clear();
    }

    /// Process an error
    pub fn process_error(&self, error: DxLinkError) {
        let listeners = self.error_listeners.lock().unwrap();
        if listeners.is_empty() {
            error!("Unhandled error in channel#{}: {:?}", self.id, error);
            return;
        }

        for wrapper in listeners.iter() {
            (wrapper.callback)(&error);
        }
    }

    // Private helper methods

    fn set_state(&self, new_state: DxLinkChannelState) {
        let mut state = self.state.lock().unwrap();
        if *state == new_state {
            return;
        }

        let prev_state = *state;
        *state = new_state;

        // Notify listeners
        for wrapper in self.state_listeners.lock().unwrap().iter() {
            (wrapper.callback)(&new_state, &prev_state);
        }
    }

    fn clear(&self) {
        self.message_listeners.lock().unwrap().clear();
        self.state_listeners.lock().unwrap().clear();
        // Note: Not clearing error listeners as per TODO in TypeScript
    }
}

// Implementing the trait
impl DxLinkChannel for DxLinkWebSocketChannel {
    fn clone_box(&self) -> Box<dyn DxLinkChannel + Send + Sync> {
        Box::new(Self {
            id: self.id,
            service: self.service.clone(),
            parameters: self.parameters.clone(),
            state: self.state.clone(),
            message_sender: self.message_sender.clone(),
            message_listeners: self.message_listeners.clone(),
            state_listeners: self.state_listeners.clone(),
            error_listeners: self.error_listeners.clone(),
        })
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn service(&self) -> &str {
        &self.service
    }

    fn parameters(&self) -> HashMap<String, Value> {
        // Convert serde_json::Value to HashMap<String, serde_json::Value>
        match serde_json::from_value(self.parameters.clone()) {
            Ok(params) => params,
            Err(_) => HashMap::new(), // Return an empty map on conversion failure
        }
    }

    fn send(&self, message: channel::DxLinkChannelMessage) {
        let this = self.clone();
        let adapted_message = DxLinkChannelMessage {
            message_type: message.message_type,
            payload: message.payload,
        };
        tokio::spawn(async move {
            if let Err(e) = this.send(adapted_message).await {
                error!("Failed to send message: {}", e);
            }
        });
    }

    /// Add a message listener
    fn add_message_listener(&self, listener: channel::ChannelMessageListener) {
        let wrapper = CallbackWrapper {
            id: Uuid::new_v4(),
            callback: Arc::new(listener),
        };
        self.message_listeners.lock().unwrap().insert(wrapper);
    }

    /// Remove a message listener
    fn remove_message_listener(&mut self, _listener: ChannelMessageListener) {
        // TODO: Implement removal by comparing function pointers or using an ID system
    }

    fn state(&self) -> channel::DxLinkChannelState {
        *self.state.lock().unwrap()
    }

    /// Add a state change listener
    fn add_state_change_listener(&mut self, listener: channel::ChannelStateChangeListener) {
        let wrapper = CallbackWrapper {
            id: Uuid::new_v4(),
            callback: Arc::new(listener),
        };
        self.state_listeners.lock().unwrap().insert(wrapper);
    }

    /// Remove a state change listener
    fn remove_state_change_listener(&mut self, _listener: channel::ChannelStateChangeListener) {
        // TODO: Implement removal logic, similar adaptation as in add_state_change_listener may be needed
    }

    /// Add an error listener
    fn add_error_listener(&mut self, listener: channel::ChannelErrorListener) {
        let wrapper = CallbackWrapper {
            id: Uuid::new_v4(),
            callback: Arc::new(listener),
        };
        self.error_listeners.lock().unwrap().insert(wrapper);
    }

    /// Remove an error listener
    fn remove_error_listener(&mut self, _listener: channel::ChannelErrorListener) {
        // TODO: Implement removal
    }

    fn close(&mut self) {
        let self_clone = self.clone();
        tokio::spawn(async move {
            if let Err(e) = self_clone.close().await {
                error!("Error closing channel: {}", e);
            }
        });
    }
}

impl Clone for DxLinkWebSocketChannel {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            service: self.service.clone(),
            parameters: self.parameters.clone(),
            state: self.state.clone(),
            message_sender: self.message_sender.clone(),
            message_listeners: self.message_listeners.clone(),
            state_listeners: self.state_listeners.clone(),
            error_listeners: self.error_listeners.clone(),
        }
    }
}

impl DxLinkChannel for Arc<DxLinkWebSocketChannel> {
    fn clone_box(&self) -> Box<dyn DxLinkChannel + Send + Sync> {
        Box::new(self.clone())
    }

    fn id(&self) -> u64 {
        (**self).id()
    }

    fn service(&self) -> &str {
        (**self).service()
    }

    fn parameters(&self) -> HashMap<String, Value> {
        (**self).parameters()
    }

    fn send(&self, message: channel::DxLinkChannelMessage) {
        let this = (**self).clone();
        let payload = serde_json::to_value(message.payload).expect("Failed to serialize payload");
        let adapted_message = DxLinkChannelMessage {
            message_type: message.message_type,
            payload,
        };
        tokio::spawn(async move {
            if let Err(e) = this.send(adapted_message).await {
                error!("Failed to send message: {}", e);
            }
        });
    }

    fn add_message_listener(&self, listener: channel::ChannelMessageListener) {
        self.as_ref().add_message_listener(listener);
    }

    fn remove_message_listener(&mut self, listener: channel::ChannelMessageListener) {
        Arc::get_mut(self)
            .unwrap()
            .remove_message_listener(listener)
    }

    fn state(&self) -> channel::DxLinkChannelState {
        (**self).state()
    }

    fn add_state_change_listener(&mut self, listener: channel::ChannelStateChangeListener) {
        Arc::get_mut(self)
            .unwrap()
            .add_state_change_listener(listener)
    }
    fn remove_state_change_listener(&mut self, listener: channel::ChannelStateChangeListener) {
        Arc::get_mut(self)
            .unwrap()
            .remove_state_change_listener(listener)
    }
    fn add_error_listener(&mut self, listener: channel::ChannelErrorListener) {
        Arc::get_mut(self).unwrap().add_error_listener(listener)
    }

    fn remove_error_listener(&mut self, listener: channel::ChannelErrorListener) {
        Arc::get_mut(self).unwrap().remove_error_listener(listener)
    }

    fn close(&mut self) {
        Arc::get_mut(self).unwrap().close()
    }
}

#[cfg(test)]
mod tests {
    use crate::DxLinkErrorType;

    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_channel_lifecycle() {
        let (tx, _rx) = mpsc::channel(32);
        let config = DxLinkWebSocketClientConfig::default();
        let channel =
            DxLinkWebSocketChannel::new(1, "test".to_string(), serde_json::json!({}), tx, &config);

        assert_eq!(channel.state(), DxLinkChannelState::Requested);

        channel.process_status_opened();
        assert_eq!(channel.state(), DxLinkChannelState::Opened);

        channel.process_status_closed();
        assert_eq!(channel.state(), DxLinkChannelState::Closed);
    }

    #[tokio::test]
    async fn test_listeners() {
        let (tx, _rx) = mpsc::channel(32);
        let config = DxLinkWebSocketClientConfig::default();
        let mut channel =
            DxLinkWebSocketChannel::new(1, "test".to_string(), serde_json::json!({}), tx, &config);

        let message_received = Arc::new(Mutex::new(false));
        let message_received_clone = message_received.clone();
        channel.add_message_listener(Box::new(move |_msg| {
            *message_received_clone.lock().unwrap() = true;
        }));

        let state_changed = Arc::new(Mutex::new(false));
        let state_changed_clone = state_changed.clone();
        channel.add_state_change_listener(Box::new(move |_new, _old| {
            *state_changed_clone.lock().unwrap() = true;
        }));

        let error_received = Arc::new(Mutex::new(false));
        let error_received_clone = error_received.clone();
        channel.add_error_listener(Box::new(move |_err| {
            *error_received_clone.lock().unwrap() = true;
        }));

        // Test state change
        channel.process_status_opened();
        assert!(*state_changed.lock().unwrap());

        // Test error
        channel.process_error(DxLinkError {
            error_type: DxLinkErrorType::Unknown,
            message: "test".to_string(),
        });
        assert!(*error_received.lock().unwrap());

        // Test message
        // TODO: Test message handling once implemented
    }
}
