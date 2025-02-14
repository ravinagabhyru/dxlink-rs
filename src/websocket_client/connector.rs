use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message as WsMessage, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, warn};

use crate::websocket_client::messages::{Message, MessageType};

type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;
// use std::hash::{Hash, Hasher};

// #[derive(Clone)]
// struct CallbackWrapper<T> {
//     id: u64,
//     callback: Arc<T>,
// }

// impl<T> PartialEq for CallbackWrapper<T> {
//     fn eq(&self, other: &Self) -> bool {
//         self.id == other.id
//     }
// }

// impl<T> Eq for CallbackWrapper<T> {}

// impl<T> Hash for CallbackWrapper<T> {
//     fn hash<H: Hasher>(&self, state: &mut H) {
//         self.id.hash(state);
//     }
// }
type OpenListener = Box<dyn Fn() + Send + Sync>;
type CloseListener = Box<dyn Fn(String, bool) + Send + Sync>;
type MessageListener = Box<dyn Fn(Box<dyn Message + Send + Sync>) + Send + Sync>;

/// Connector for managing WebSocket connection
#[derive(Clone)]
pub struct WebSocketConnector {
    url: String,
    write_stream: Arc<Mutex<Option<SplitSink<WebSocket, WsMessage>>>>,
    read_stream: Arc<Mutex<Option<futures_util::stream::SplitStream<WebSocket>>>>,
    is_available: Arc<Mutex<bool>>,
    open_listener: Arc<Mutex<Option<OpenListener>>>,
    close_listener: Arc<Mutex<Option<CloseListener>>>,
    message_listener: Arc<Mutex<Option<MessageListener>>>,
}

impl WebSocketConnector {
    /// Create a new WebSocket connector
    pub fn new(url: String) -> Self {
        Self {
            url,
            write_stream: Arc::new(Mutex::new(None)),
            read_stream: Arc::new(Mutex::new(None)),
            is_available: Arc::new(Mutex::new(false)),
            open_listener: Arc::new(Mutex::new(None)),
            close_listener: Arc::new(Mutex::new(None)),
            message_listener: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the WebSocket connection
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.write_stream.lock().await.is_some() {
            return Ok(());
        }

        let (ws_stream, _) = connect_async(&self.url).await?;
        let (write, read) = ws_stream.split();

        *self.write_stream.lock().await = Some(write);
        *self.read_stream.lock().await = Some(read);
        *self.is_available.lock().await = true;

        let connector = self.clone();
        tokio::spawn(async move {
            if let Err(e) = connector.handle_connection().await {
                error!("Connection handler error: {}", e);
            }
        });

        Ok(())
    }

    /// Stop the WebSocket connection
    pub async fn stop(&self) {
        if let Some(mut write) = self.write_stream.lock().await.take() {
            if let Err(e) = write.send(WsMessage::Close(None)).await {
                error!("Error closing WebSocket: {}", e);
            }
        }
        *self.is_available.lock().await = false;
    }

    /// Send a message through the WebSocket connection
    pub async fn send_message(
        &self,
        message: Box<dyn Message + Send + Sync>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !*self.is_available.lock().await {
            return Ok(());
        }

        let json = serde_json::to_string(&message.payload())?;
        debug!("Sending WebSocket message: {}", json);

        if let Some(write) = self.write_stream.lock().await.as_mut() {
            write.send(WsMessage::Text(json)).await?;
            debug!("Message sent successfully");
        } else {
            warn!("No write stream available");
        }

        Ok(())
    }

    /// Set the listener for connection open events
    pub async fn set_open_listener(&self, listener: OpenListener) {
        *self.open_listener.lock().await = Some(listener);
    }

    /// Set the listener for connection close events
    pub async fn set_close_listener(&self, listener: CloseListener) {
        *self.close_listener.lock().await = Some(listener);
    }

    /// Set the listener for incoming messages
    pub async fn set_message_listener(&self, listener: MessageListener) {
        *self.message_listener.lock().await = Some(listener);
    }

    /// Get the WebSocket URL
    pub fn url(&self) -> &str {
        &self.url
    }

    async fn handle_message(
        msg: WsMessage,
        message_listener: Arc<Mutex<Option<MessageListener>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let WsMessage::Text(text) = msg {
            debug!("Received WebSocket message: {}", text);
            let value: Value = serde_json::from_str(&text)?;

            // Validate message structure
            if !value.is_object() {
                error!("Message is not an object: {}", text);
                return Err("Message is not an object".into());
            }

            let msg_type = value["type"].as_str().ok_or_else(|| {
                error!("Missing or invalid message type in: {}", text);
                "Missing or invalid message type"
            })?;
            let channel = value["channel"].as_u64().ok_or_else(|| {
                error!("Missing or invalid channel in: {}", text);
                "Missing or invalid channel"
            })?;

            debug!(
                "Processing message type: {} for channel: {} payload: {}",
                msg_type, channel, text
            );
            debug!("Value: {}", value);

            // Create appropriate message type based on the message type field
            let message = match msg_type {
                "SETUP" => MessageType::Setup(serde_json::from_value(value.clone())?),
                "AUTH_STATE" => MessageType::AuthState(serde_json::from_value(value.clone())?),
                "CHANNEL_OPENED" => {
                    MessageType::ChannelOpened(serde_json::from_value(value.clone())?)
                }
                "CHANNEL_CLOSED" => {
                    MessageType::ChannelClosed(serde_json::from_value(value.clone())?)
                }
                // ... other message types
                _ => return Err("Unknown message type".into()),
            };

            if let Some(listener) = message_listener.lock().await.as_ref() {
                listener(Box::new(message));
            }

            // match serde_json::from_value::<MessageType>(value) {
            //     Ok(msg) => {
            //         debug!("Message before forwarding to listener: type={}", msg.message_type());
            //         if let Some(listener) = message_listener.lock().await.as_ref() {
            //             debug!("Forwarding message to listener: type={}", msg.message_type());
            //             listener(Box::new(msg));
            //         } else {
            //             warn!("No message listener set for message: {}", text);
            //         }
            //     },
            //     Err(e) => {
            //         error!("Failed to parse message: {}: {}", e, text);
            //     }
            // };
        } else {
            debug!("Received non-text WebSocket message: {:?}", msg);
        }

        Ok(())
    }

    async fn handle_connection(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut read_guard = self.read_stream.lock().await;
        let read = read_guard.as_mut().ok_or("No read stream available")?;

        // Notify that connection is open
        if let Some(listener) = self.open_listener.lock().await.as_ref() {
            debug!("Notifying open listener");
            listener();
        } else {
            debug!("No open listener registered");
        }

        // Set connection as available before processing messages
        *self.is_available.lock().await = true;
        debug!("Connection is now available");

        // Process messages until connection is closed
        while let Some(result) = read.next().await {
            match result {
                Ok(message) => {
                    debug!("Received WebSocket message: {:?}", message);
                    if let Err(e) =
                        Self::handle_message(message, self.message_listener.clone()).await
                    {
                        error!("Error handling message: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    if let Some(listener) = self.close_listener.lock().await.as_ref() {
                        listener(e.to_string(), true);
                    }
                    break;
                }
            }
        }

        // Connection is now closed
        *self.is_available.lock().await = false;
        if let Some(listener) = self.close_listener.lock().await.as_ref() {
            listener("Connection closed".to_string(), false);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Notify;

    #[tokio::test]
    async fn test_connector_lifecycle() {
        let connector = WebSocketConnector::new("ws://localhost:8080".to_string());

        let open_notify = Arc::new(Notify::new());
        let open_notify_clone = open_notify.clone();

        connector
            .set_open_listener(Box::new(move || {
                open_notify_clone.notify_one();
            }))
            .await;

        // Note: This test requires a running WebSocket server
        // For real testing, we would use a mock server
        if let Ok(()) = connector.start().await {
            open_notify.notified().await;
            assert!(*connector.is_available.lock().await);

            connector.stop().await;
            assert!(!*connector.is_available.lock().await);
        }
    }
}
