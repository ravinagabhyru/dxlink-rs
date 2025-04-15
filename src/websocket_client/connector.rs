use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message as WsMessage, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, warn};

use crate::feed::messages::FeedConfigMessage;
use crate::feed::messages::FeedDataMessage;
use crate::DomConfigMessage;
use crate::DomSnapshotMessage;
use crate::websocket_client::messages::{
    AuthStateMessage, ChannelClosedMessage, ChannelOpenedMessage,
    ErrorMessage, KeepaliveMessage,
    Message, MessageType, SetupMessage,
};

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

            // 1. Parse the incoming text into a generic serde_json::Value
            let value: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    error!("Failed to parse JSON message: {}: {}", e, text);
                    return Err(format!("Failed to parse JSON message: {}", e).into());
                }
            };

            // Validate message structure
            if !value.is_object() {
                error!("Message is not an object: {}", text);
                return Err("Message is not an object".into());
            }

            // 2. Extract the type and channel fields
            let msg_type = match value.get("type").and_then(|t| t.as_str()) {
                Some(t) => t,
                None => {
                    error!("Missing or invalid message type in: {}", text);
                    return Err("Missing or invalid message type".into());
                }
            };

            let channel = match value.get("channel").and_then(|c| c.as_u64()) {
                Some(c) => c,
                None => {
                    error!("Missing or invalid channel in: {}", text);
                    return Err("Missing or invalid channel".into());
                }
            };

            debug!(
                "Processing message type: {} for channel: {}",
                msg_type, channel
            );

            // 3. Use a match statement on the extracted type string
            let message_result: Result<Box<dyn Message + Send + Sync>, serde_json::Error> = match msg_type {
                // Connection/authentication messages
                "SETUP" => serde_json::from_value::<SetupMessage>(value.clone())
                    .map(|m| Box::new(MessageType::Setup(m)) as Box<dyn Message + Send + Sync>),

                "AUTH_STATE" => serde_json::from_value::<AuthStateMessage>(value.clone())
                    .map(|m| Box::new(MessageType::AuthState(m)) as Box<dyn Message + Send + Sync>),

                "KEEPALIVE" => serde_json::from_value::<KeepaliveMessage>(value.clone())
                    .map(|m| Box::new(MessageType::KeepAlive(m)) as Box<dyn Message + Send + Sync>),

                // Channel lifecycle messages
                "CHANNEL_OPENED" => serde_json::from_value::<ChannelOpenedMessage>(value.clone())
                    .map(|m| Box::new(MessageType::ChannelOpened(m)) as Box<dyn Message + Send + Sync>),

                "CHANNEL_CLOSED" => serde_json::from_value::<ChannelClosedMessage>(value.clone())
                    .map(|m| Box::new(MessageType::ChannelClosed(m)) as Box<dyn Message + Send + Sync>),

                "ERROR" => serde_json::from_value::<ErrorMessage>(value.clone())
                    .map(|m| Box::new(MessageType::Error(m)) as Box<dyn Message + Send + Sync>),

                // FEED messages
                "FEED_CONFIG" => serde_json::from_value::<FeedConfigMessage>(value.clone())
                    .map(|m| Box::new(MessageType::FeedConfig(m)) as Box<dyn Message + Send + Sync>),

                "FEED_DATA" => serde_json::from_value::<FeedDataMessage>(value.clone())
                    .map(|m| Box::new(MessageType::FeedData(m)) as Box<dyn Message + Send + Sync>),

                // DOM messages
                "DOM_CONFIG" => serde_json::from_value::<DomConfigMessage>(value.clone())
                    .map(|m| Box::new(MessageType::DomConfig(m)) as Box<dyn Message + Send + Sync>),

                "DOM_SNAPSHOT" => serde_json::from_value::<DomSnapshotMessage>(value.clone())
                    .map(|m| Box::new(MessageType::DomSnapshot(m)) as Box<dyn Message + Send + Sync>),

                // Unknown message type
                _ => {
                    warn!("Unknown message type received: {}", msg_type);
                    return Err(format!("Unknown message type: {}", msg_type).into());
                }
            };

            // 4. Process the deserialization result
            let message = match message_result {
                Ok(msg) => msg,
                Err(e) => {
                    error!("Failed to deserialize {} message: {}: {}", msg_type, e, text);
                    return Err(format!("Failed to deserialize {} message: {}", msg_type, e).into());
                }
            };

            // 5. Pass the boxed message to the message_listener
            if let Some(listener) = message_listener.lock().await.as_ref() {
                debug!("Forwarding message to listener: type={}", message.message_type());
                listener(message);
            } else {
                warn!("No message listener set for message: {}", text);
            }
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
    use crate::{DxLinkErrorType, FeedDataFormat};

    use super::*;
    use std::sync::Arc;
    use tokio::sync::{mpsc, Notify};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

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

    #[tokio::test]
    async fn test_handle_message_valid_setup() {
        // Create a channel to receive the parsed message
        let (tx, mut rx) = mpsc::channel(1);

        // Create a message listener that will send the parsed message through the channel
        let message_listener: MessageListener = Box::new(move |msg| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(msg).await;
            });
        });

        let message_listener = Arc::new(Mutex::new(Some(message_listener)));

        // Create a valid SETUP message
        let setup_json = r#"{"type":"SETUP","channel":0,"version":"1.0","keepaliveTimeout":30000}"#;
        let ws_message = WsMessage::Text(setup_json.to_string());

        // Process the message
        let result = WebSocketConnector::handle_message(ws_message, message_listener).await;

        // Verify the message was processed successfully
        assert!(result.is_ok(), "handle_message returned an error: {:?}", result);

        // Verify the message was forwarded to the listener
        let received_msg = rx.recv().await.expect("No message received from listener");

        // Verify the message type is correct
        assert_eq!(received_msg.message_type(), "SETUP");

        // Verify the message is a SetupMessage
        let msg_any = received_msg.as_any();
        let msg_type = msg_any.downcast_ref::<MessageType>().expect("Not a MessageType");

        if let MessageType::Setup(setup_msg) = msg_type {
            assert_eq!(setup_msg.channel, 0);
            assert_eq!(setup_msg.version, "1.0");
            assert_eq!(setup_msg.keepalive_timeout, Some(30000));
        } else {
            panic!("Not a SetupMessage");
        }
    }

    #[tokio::test]
    async fn test_handle_message_valid_feed_config() {
        // Create a channel to receive the parsed message
        let (tx, mut rx) = mpsc::channel(1);

        // Create a message listener
        let message_listener: MessageListener = Box::new(move |msg| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(msg).await;
            });
        });

        let message_listener = Arc::new(Mutex::new(Some(message_listener)));

        // Create a valid FEED_CONFIG message
        let feed_config_json = r#"{"type":"FEED_CONFIG","channel":1,"aggregationPeriod":1000,"dataFormat":"FULL"}"#;
        let ws_message = WsMessage::Text(feed_config_json.to_string());

        // Process the message
        let result = WebSocketConnector::handle_message(ws_message, message_listener).await;

        // Verify the message was processed successfully
        assert!(result.is_ok(), "handle_message returned an error: {:?}", result);

        // Verify the message was forwarded to the listener
        let received_msg = rx.recv().await.expect("No message received from listener");

        // Verify the message type is correct
        assert_eq!(received_msg.message_type(), "FEED_CONFIG");

        // Verify the message is a FeedConfigMessage
        let msg_any = received_msg.as_any();
        let msg_type = msg_any.downcast_ref::<MessageType>().expect("Not a MessageType");

        if let MessageType::FeedConfig(feed_config_msg) = msg_type {
            assert_eq!(feed_config_msg.channel, 1);
            assert_eq!(feed_config_msg.aggregation_period, 1000);
            assert!(matches!(feed_config_msg.data_format, FeedDataFormat::Full));
        } else {
            panic!("Not a FeedConfigMessage");
        }
    }

    #[tokio::test]
    async fn test_handle_message_valid_dom_snapshot() {
        // Create a channel to receive the parsed message
        let (tx, mut rx) = mpsc::channel(1);

        // Create a message listener
        let message_listener: MessageListener = Box::new(move |msg| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(msg).await;
            });
        });

        let message_listener = Arc::new(Mutex::new(Some(message_listener)));

        // Create a valid DOM_SNAPSHOT message
        let dom_snapshot_json = r#"{"type":"DOM_SNAPSHOT","channel":2,"time":1617293142123,"bids":[{"price":100.5,"size":10.0},{"price":100.0,"size":20.0}],"asks":[{"price":101.0,"size":15.0},{"price":101.5,"size":25.0}]}"#;
        let ws_message = WsMessage::Text(dom_snapshot_json.to_string());

        // Process the message
        let result = WebSocketConnector::handle_message(ws_message, message_listener).await;

        // Verify the message was processed successfully
        assert!(result.is_ok(), "handle_message returned an error: {:?}", result);

        // Verify the message was forwarded to the listener
        let received_msg = rx.recv().await.expect("No message received from listener");

        // Verify the message type is correct
        assert_eq!(received_msg.message_type(), "DOM_SNAPSHOT");

        // Verify the message is a DomSnapshotMessage
        let msg_any = received_msg.as_any();
        let msg_type = msg_any.downcast_ref::<MessageType>().expect("Not a MessageType");

        if let MessageType::DomSnapshot(dom_snapshot_msg) = msg_type {
            assert_eq!(dom_snapshot_msg.channel, 2);
            assert_eq!(dom_snapshot_msg.time, 1617293142123);
            assert_eq!(dom_snapshot_msg.bids.len(), 2);
            assert_eq!(dom_snapshot_msg.asks.len(), 2);
            assert_eq!(dom_snapshot_msg.bids[0].price, 100.5);
            assert_eq!(dom_snapshot_msg.bids[0].size, 10.0);
            assert_eq!(dom_snapshot_msg.asks[0].price, 101.0);
            assert_eq!(dom_snapshot_msg.asks[0].size, 15.0);
        } else {
            panic!("Not a DomSnapshotMessage");
        }
    }

    #[tokio::test]
    async fn test_handle_message_invalid_json() {
        // Create a message listener that shouldn't be called
        let message_listener: MessageListener = Box::new(move |_| {
            panic!("Message listener should not be called for invalid JSON");
        });

        let message_listener = Arc::new(Mutex::new(Some(message_listener)));

        // Create an invalid JSON message
        let invalid_json = r#"{"type":"SETUP","channel":0,invalid_json"#;
        let ws_message = WsMessage::Text(invalid_json.to_string());

        // Process the message
        let result = WebSocketConnector::handle_message(ws_message, message_listener).await;

        // Verify that an error was returned
        assert!(result.is_err(), "handle_message should return an error for invalid JSON");
    }

    #[tokio::test]
    async fn test_handle_message_missing_fields() {
        // Create a message listener that shouldn't be called
        let message_listener: MessageListener = Box::new(move |_| {
            panic!("Message listener should not be called for messages with missing fields");
        });

        let message_listener = Arc::new(Mutex::new(Some(message_listener)));

        // Create messages with missing fields

        // Missing type
        let missing_type = r#"{"channel":0,"version":"1.0"}"#;
        let ws_message = WsMessage::Text(missing_type.to_string());
        let result = WebSocketConnector::handle_message(ws_message, message_listener.clone()).await;
        assert!(result.is_err(), "handle_message should return an error for missing type");

        // Missing channel
        let missing_channel = r#"{"type":"SETUP","version":"1.0"}"#;
        let ws_message = WsMessage::Text(missing_channel.to_string());
        let result = WebSocketConnector::handle_message(ws_message, message_listener.clone()).await;
        assert!(result.is_err(), "handle_message should return an error for missing channel");
    }

    #[tokio::test]
    async fn test_handle_message_unknown_type() {
        // Create a message listener that shouldn't be called
        let message_listener: MessageListener = Box::new(move |_| {
            panic!("Message listener should not be called for unknown message types");
        });

        let message_listener = Arc::new(Mutex::new(Some(message_listener)));

        // Create a message with an unknown type
        let unknown_type = r#"{"type":"UNKNOWN_TYPE","channel":0}"#;
        let ws_message = WsMessage::Text(unknown_type.to_string());

        // Process the message
        let result = WebSocketConnector::handle_message(ws_message, message_listener).await;

        // Verify that an error was returned
        assert!(result.is_err(), "handle_message should return an error for unknown message types");
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("Unknown message type"), "Error message should indicate unknown type: {}", err_str);
    }

    #[tokio::test]
    async fn test_handle_message_invalid_field_values() {
        // Create a message listener that shouldn't be called
        let message_listener: MessageListener = Box::new(move |_| {
            panic!("Message listener should not be called for messages with invalid fields");
        });

        let message_listener = Arc::new(Mutex::new(Some(message_listener)));

        // Create a message with invalid field values (wrong types)
        let invalid_field = r#"{"type":"SETUP","channel":"not-a-number","version":"1.0"}"#;
        let ws_message = WsMessage::Text(invalid_field.to_string());

        // Process the message
        let result = WebSocketConnector::handle_message(ws_message, message_listener).await;

        // Verify that an error was returned
        assert!(result.is_err(), "handle_message should return an error for invalid field values");
    }

    // Additional tests for Task 10: Testing handle_message for multiple message types and error cases

    #[tokio::test]
    async fn test_handle_message_auth_state() {
        // Create a channel to receive the parsed message
        let (tx, mut rx) = mpsc::channel(1);

        // Create a message listener
        let message_listener: MessageListener = Box::new(move |msg| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(msg).await;
            });
        });

        let message_listener = Arc::new(Mutex::new(Some(message_listener)));

        // Create a valid AUTH_STATE message
        let auth_state_json = r#"{"type":"AUTH_STATE","channel":0,"state":"AUTHORIZED"}"#;
        let ws_message = WsMessage::Text(auth_state_json.to_string());

        // Process the message
        let result = WebSocketConnector::handle_message(ws_message, message_listener).await;

        // Verify the message was processed successfully
        assert!(result.is_ok(), "handle_message returned an error: {:?}", result);

        // Verify the message was forwarded to the listener
        let received_msg = rx.recv().await.expect("No message received from listener");

        // Verify the message type is correct
        assert_eq!(received_msg.message_type(), "AUTH_STATE");

        // Verify the message is an AuthStateMessage
        let msg_any = received_msg.as_any();
        let msg_type = msg_any.downcast_ref::<MessageType>().expect("Not a MessageType");

        if let MessageType::AuthState(auth_state_msg) = msg_type {
            assert_eq!(auth_state_msg.channel, 0);
            assert_eq!(auth_state_msg.state, "AUTHORIZED");
        } else {
            panic!("Not an AuthStateMessage");
        }
    }

    #[tokio::test]
    async fn test_handle_message_keepalive() {
        // Create a channel to receive the parsed message
        let (tx, mut rx) = mpsc::channel(1);

        // Create a message listener
        let message_listener: MessageListener = Box::new(move |msg| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(msg).await;
            });
        });

        let message_listener = Arc::new(Mutex::new(Some(message_listener)));

        // Create a valid KEEPALIVE message
        let keepalive_json = r#"{"type":"KEEPALIVE","channel":0}"#;
        let ws_message = WsMessage::Text(keepalive_json.to_string());

        // Process the message
        let result = WebSocketConnector::handle_message(ws_message, message_listener).await;

        // Verify the message was processed successfully
        assert!(result.is_ok(), "handle_message returned an error: {:?}", result);

        // Verify the message was forwarded to the listener
        let received_msg = rx.recv().await.expect("No message received from listener");

        // Verify the message type is correct
        assert_eq!(received_msg.message_type(), "KEEPALIVE");

        // Verify the message is a KeepaliveMessage
        let msg_any = received_msg.as_any();
        let msg_type = msg_any.downcast_ref::<MessageType>().expect("Not a MessageType");

        if let MessageType::KeepAlive(keepalive_msg) = msg_type {
            assert_eq!(keepalive_msg.channel, 0);
        } else {
            panic!("Not a KeepaliveMessage");
        }
    }

    #[tokio::test]
    async fn test_handle_message_error_message() {
            // Create a channel to receive the parsed message
            let (tx, mut rx) = mpsc::channel(1);

            // Create a message listener
            let message_listener: MessageListener = Box::new(move |msg| {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(msg).await;
                });
            });

            let message_listener = Arc::new(Mutex::new(Some(message_listener)));

            // Create a valid ERROR message with the correct enum value (lowercase due to #[serde(rename_all = "snake_case")])
            let error_json = r#"{"type":"ERROR","channel":0,"error":"timeout","message":"Connection timed out"}"#;
            let ws_message = WsMessage::Text(error_json.to_string());

            // Process the message
            let result = WebSocketConnector::handle_message(ws_message, message_listener).await;

            // Verify the message was processed successfully
            assert!(result.is_ok(), "handle_message returned an error: {:?}", result);

            // Verify the message was forwarded to the listener
            let received_msg = rx.recv().await.expect("No message received from listener");

            // Verify the message type is correct
            assert_eq!(received_msg.message_type(), "ERROR");

            // Verify the message is an ErrorMessage
            let msg_any = received_msg.as_any();
            let msg_type = msg_any.downcast_ref::<MessageType>().expect("Not a MessageType");

            if let MessageType::Error(error_msg) = msg_type {
                assert_eq!(error_msg.channel, 0);
                // Check the error type as a string representation
                assert_eq!(error_msg.error, DxLinkErrorType::Timeout);
                assert_eq!(error_msg.message, "Connection timed out");
            } else {
                panic!("Not an ErrorMessage");
            }
        }
}
