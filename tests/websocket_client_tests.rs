use std::collections::HashMap;
use serde_json::json;
use dxlink_rs::{
    core::{
        auth::DxLinkAuthState,
        channel::{DxLinkChannelMessage, DxLinkChannelState},
        client::DxLinkConnectionState
    },
    websocket_client::{
        client::DxLinkWebSocketClient,
        config::DxLinkWebSocketClientConfig,
        messages::{MessageType, ChannelClosedMessage, ChannelOpenedMessage},
    },
};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

const DEMO_DXLINK_WS_URL: &str = "wss://demo.dxfeed.com/dxlink-ws";

// Helper function to initialize logging for tests
fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
}

// Helper function to create and initialize a client
async fn create_client() -> DxLinkWebSocketClient {
    let config = DxLinkWebSocketClientConfig::default();
    let mut client = DxLinkWebSocketClient::new(config);

    // Set up connection and authentication state listeners
    let (conn_tx, mut conn_rx) = mpsc::channel(32);
    client.add_connection_state_change_listener(Box::new({
        let tx = conn_tx.clone();
        move |new_state: &DxLinkConnectionState, _old: &DxLinkConnectionState| {
            let new_state = *new_state;
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(new_state).await.unwrap();
            });
        }
    })).await;

    let (auth_tx, mut auth_rx) = mpsc::channel(32);
    client.add_auth_state_change_listener(Box::new({
        let tx = auth_tx.clone();
        move |new_state: &DxLinkAuthState, _old: &DxLinkAuthState| {
            let new_state = *new_state;
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(new_state).await.unwrap();
            });
        }
    })).await;

    // Connect to server
    let con = client.connect(DEMO_DXLINK_WS_URL.to_string()).await;
    assert!(con.is_ok());
    tracing::debug!("Connected to server");

    // Wait for Connecting state
    let state = timeout(Duration::from_secs(5), conn_rx.recv())
        .await
        .expect("Timeout waiting for connecting state")
        .unwrap();
    assert_eq!(state, DxLinkConnectionState::Connecting);

    // Wait for Connected state
    let state = timeout(Duration::from_secs(5), conn_rx.recv())
        .await
        .expect("Timeout waiting for connected state")
        .unwrap();
    assert_eq!(state, DxLinkConnectionState::Connected);

    // Wait for Authorized state
    let state = timeout(Duration::from_secs(5), auth_rx.recv())
        .await
        .expect("Timeout waiting for authorized state")
        .unwrap();
    assert_eq!(state, DxLinkAuthState::Authorized);

    // Make sure we are in a good state before proceeding
    wait_for_states(&client).await;
    tracing::debug!("Client states verified");

    client
}

// Helper function to wait for expected client states
async fn wait_for_states(client: &DxLinkWebSocketClient) {
    assert_eq!(client.get_connection_state().await, DxLinkConnectionState::Connected);
    assert_eq!(client.get_auth_state().await, DxLinkAuthState::Authorized);
}

// /*
// Added extension trait to allow downcasting DxLinkChannel objects
// */
// trait DxLinkChannelAsAny {
//     fn as_any(&self) -> &dyn std::any::Any;
// }

// // Only implement for Arc<DxLinkWebSocketChannel> since that's what we need
// impl DxLinkChannelAsAny for Box<dyn DxLinkChannel + Send + Sync> {
//     fn as_any(&self) -> &dyn std::any::Any {
//         self
//     }
// }

#[tokio::test]
async fn test_connection_setup() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    tracing::info!("Starting connection setup test");

    let config = DxLinkWebSocketClientConfig::default();
    let mut client = DxLinkWebSocketClient::new(config);

    let (tx, mut rx) = mpsc::channel(32);

    client
        .add_connection_state_change_listener(Box::new({
            let tx = tx.clone();
            move |new_state: &DxLinkConnectionState, old_state: &DxLinkConnectionState| {
                tracing::debug!("Connection state changed from {:?} to {:?}", old_state, new_state);
                let new_state = *new_state;
                let tx = tx.clone();
                // Spawn a new async task to send the state change
                tokio::spawn(async move {
                    if let Err(e) = tx.send(new_state).await {
                        tracing::error!("Failed to send state change: {}", e);
                    }
                });
            }
        }))
        .await;

    let con = client.connect(DEMO_DXLINK_WS_URL.to_string()).await;
    assert!(con.is_ok());
    tracing::debug!("Connect call completed");

    // Expect Connecting state
    let state = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("Timeout waiting for connecting state")
        .unwrap();
    tracing::debug!("Received connecting state: {:?}", state);
    assert_eq!(state, DxLinkConnectionState::Connecting);

    // Expect Connected state
    let state = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("Timeout waiting for connected state")
        .unwrap();
    tracing::debug!("Received connected state: {:?}", state);
    assert_eq!(state, DxLinkConnectionState::Connected);

    let _ = client.disconnect().await;

    // Expect NotConnected state
    let state = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("Timeout waiting for not connected state")
        .unwrap();
    assert_eq!(state, DxLinkConnectionState::NotConnected);
}


#[tokio::test]
async fn test_authentication() {
    let config = DxLinkWebSocketClientConfig::default();
    let mut client = DxLinkWebSocketClient::new(config);

    let (tx, mut rx) = mpsc::channel(32);

    client
            .add_auth_state_change_listener(Box::new({
                let tx = tx.clone();
                move |new_state: &DxLinkAuthState, _old: &DxLinkAuthState| {
                    let new_state = *new_state;
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        tx.send(new_state).await.unwrap();
                    });
                }
            })).await;
    let con = client.connect(DEMO_DXLINK_WS_URL.to_string()).await;
    assert!(con.is_ok());

    // Set the auth token
    let auth = client.set_auth_token("demo".to_string()).await;
    assert!(auth.is_ok());
    tracing::debug!("Authentication token set");

    let state = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("Timeout")
        .unwrap();
    assert!(state == DxLinkAuthState::Authorized || state == DxLinkAuthState::Unauthorized); // Could be either

    let _ = client.disconnect().await;
}

#[tokio::test]
async fn test_feed_channel_request_and_close() {
    init_logging();

    let mut client = create_client().await;
    
    // Set up state change and message channels
    let (state_tx, _state_rx) = mpsc::channel(32);
    let (msg_tx, mut msg_rx) = mpsc::channel::<MessageType>(32);

    // Set up message listener first
    let _msg_listener = Box::new({
        let tx = msg_tx.clone();
        move |msg: &DxLinkChannelMessage| {
            tracing::info!("Received channel message: {:?}", msg);
            let tx = tx.clone();
            let message = match msg.message_type.as_str() {
                "CHANNEL_OPENED" => {
                    tracing::info!("Processing CHANNEL_OPENED message");
                    let params = msg.payload.get("parameters")
                        .and_then(|v| v.as_object())
                        .map(|obj| {
                            obj.iter().map(|(k, v)| {
                                (k.clone(), v.clone())
                            }).collect::<HashMap<String, serde_json::Value>>()
                        });

                    Some(MessageType::ChannelOpened(ChannelOpenedMessage {
                        message_type: msg.message_type.clone(),
                        channel: msg.payload.get("channel")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        service: msg.payload.get("service")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        parameters: params
                    }))
                },
                "FEED_CONFIG" => {
                    tracing::info!("Processing FEED_CONFIG message");
                    serde_json::from_value(msg.payload.clone())
                        .map(|config| MessageType::FeedConfig(config))
                        .ok()
                },
                "FEED_DATA" => {
                    tracing::info!("Processing FEED_DATA message");
                    serde_json::from_value(msg.payload.clone())
                        .map(|data| MessageType::FeedData(data))
                        .ok()
                },
                "CHANNEL_CLOSED" => {
                    tracing::info!("Processing CHANNEL_CLOSED message");
                    Some(MessageType::ChannelClosed(ChannelClosedMessage {
                        message_type: msg.message_type.clone(),
                        channel: msg.payload.get("channel")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                    }))
                },
                _ => {
                    tracing::info!("Ignoring message type: {}", msg.message_type);
                    None
                }
            };
            
            if let Some(message_type) = message {
                tokio::spawn(async move {
                    tracing::info!("Sending message to channel: {:?}", message_type);
                    tx.send(message_type).await.unwrap();
                });
            }
        }
    });

    // Set up state change listener before creating the channel
    let _state_listener = Box::new({
        let tx = state_tx.clone();
        move |new_state: &DxLinkChannelState, old_state: &DxLinkChannelState| {
            tracing::debug!("Channel state changed from {:?} to {:?}", old_state, new_state);
            let new_state = *new_state;
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(new_state).await.unwrap();
            });
        }
    });

    // Create the channel
    let channel = client
        .open_channel("FEED".to_string(), json!({"contract": "AUTO"}))
        .await;
    tracing::debug!("Channel created with ID: {}", channel.id);

    // Add the message listener for subsequent messages
    let _msg_listener_id = channel.add_message_listener(Box::new({
        let tx = msg_tx.clone();
        move |msg: &DxLinkChannelMessage| {
            tracing::info!("Received channel message: {:?}", msg);
            let tx = tx.clone();
            let message = match msg.message_type.as_str() {
                "FEED_CONFIG" => {
                    tracing::info!("Processing FEED_CONFIG message");
                    serde_json::from_value(msg.payload.clone())
                        .map(|config| MessageType::FeedConfig(config))
                        .ok()
                },
                "FEED_DATA" => {
                    tracing::info!("Processing FEED_DATA message");
                    serde_json::from_value(msg.payload.clone())
                        .map(|data| MessageType::FeedData(data))
                        .ok()
                },
                "CHANNEL_CLOSED" => {
                    tracing::info!("Processing CHANNEL_CLOSED message");
                    Some(MessageType::ChannelClosed(ChannelClosedMessage {
                        message_type: msg.message_type.clone(),
                        channel: msg.payload.get("channel")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                    }))
                },
                _ => {
                    tracing::info!("Ignoring message type: {}", msg.message_type);
                    None
                }
            };
            
            if let Some(message_type) = message {
                tokio::spawn(async move {
                    tracing::info!("Sending message to channel: {:?}", message_type);
                    tx.send(message_type).await.unwrap();
                });
            }
        }
    }));

    // Wait for the channel to be opened
    let timeout_duration = Duration::from_secs(5);
    let start_time = std::time::Instant::now();
    
    while channel.state() != DxLinkChannelState::Opened {
        if start_time.elapsed() > timeout_duration {
            panic!("Timeout waiting for channel to open. Current state: {:?}", channel.state());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tracing::debug!("Channel opened successfully");

    // Add a small delay to ensure message handling is complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Set up FEED service configuration
    tracing::info!("Sending feed setup message");
    let setup_msg = DxLinkChannelMessage {
        message_type: "FEED_SETUP".to_string(),
        payload: json!({
            "type": "FEED_SETUP",
            "channel": channel.id,
            "acceptAggregationPeriod": 10,
            "acceptDataFormat": "COMPACT",
            "acceptEventFields": {
                "Quote": ["eventType", "eventSymbol", "bidPrice", "askPrice", "bidSize", "askSize"]
            }
        })
    };
    tracing::debug!("FEED_SETUP message: {:?}", setup_msg);
    let setup_result = channel.send(setup_msg).await;
    tracing::debug!("FEED_SETUP result: {:?}", setup_result);
    assert!(setup_result.is_ok(), "Failed to send FEED_SETUP message: {:?}", setup_result);
    
    // Wait for FEED_CONFIG response
    tracing::info!("Waiting for FEED_CONFIG message");
    if let Ok(Some(msg)) = timeout(Duration::from_secs(5), msg_rx.recv()).await {
        match msg {
            MessageType::FeedConfig(config) => {
                tracing::info!("Received feed config: {:?}", config);
            },
            _ => panic!("Expected FEED_CONFIG message but got: {:?}", msg),
        }
    } else {
        tracing::warn!("Timeout waiting for FEED_CONFIG message");
    }

    // Send subscription request
    tracing::info!("Sending feed subscription message");
    let sub_msg = DxLinkChannelMessage {
        message_type: "FEED_SUBSCRIPTION".to_string(),
        payload: json!({
            "type": "FEED_SUBSCRIPTION",
            "channel": channel.id,
            "add": [{ "symbol": "AAPL", "type": "Quote" }]
        })
    };
    tracing::debug!("FEED_SUBSCRIPTION message: {:?}", sub_msg);
    let sub_result = channel.send(sub_msg).await;
    tracing::debug!("FEED_SUBSCRIPTION result: {:?}", sub_result);

    // Wait for feed data
    tracing::info!("Waiting for feed data");
    let mut received_data = false;
    while !received_data {
        let received_msg = timeout(Duration::from_secs(5), msg_rx.recv()).await;
        if let Ok(Some(msg)) = received_msg {
            match msg {
                MessageType::FeedConfig(config) => {
                    tracing::info!("Received feed config: {:?}", config);
                    // Continue waiting for FEED_DATA
                },
                MessageType::FeedData(data) => {
                    tracing::info!("Received feed data: {:?}", data);
                    // Verify it's a Quote event for AAPL
                    assert_eq!(data.channel, channel.id);
                    // Further data validation could be done here if needed
                    received_data = true;
                },
                _ => panic!("Expected FEED_CONFIG or FEED_DATA message but got: {:?}", msg),
            }
        } else {
            panic!("Timeout waiting for feed data");
        }
    }

    // Clean close the channel
    tracing::info!("Closing channel");
    let close_result = channel.close().await;
    assert!(close_result.is_ok(), "Failed to close channel: {:?}", close_result);

    // Wait for channel closed message
    tracing::info!("Waiting for CHANNEL_CLOSED message");
    if let Ok(Some(msg)) = timeout(Duration::from_secs(5), msg_rx.recv()).await {
        match msg {
            MessageType::ChannelClosed(closed) => {
                assert_eq!(closed.channel, channel.id);
            },
            _ => panic!("Expected CHANNEL_CLOSED message but got: {:?}", msg),
        }
    }

    // Disconnect client
    tracing::info!("Disconnecting client");
    let _ = client.disconnect().await;
}

#[tokio::test]
async fn test_feed_subscription() {
    // Initialize logging for tests
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    tracing::info!("Starting feed subscription test");

    let config = DxLinkWebSocketClientConfig::default();
    let mut client = DxLinkWebSocketClient::new(config);

    // Set up connection state listener
    let (conn_tx, mut conn_rx) = mpsc::channel(32);
    client.add_connection_state_change_listener(Box::new({
        let tx = conn_tx.clone();
        move |new_state: &DxLinkConnectionState, _old: &DxLinkConnectionState| {
            let new_state = *new_state;
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(new_state).await.unwrap();
            });
        }
    })).await;

    // Set up auth state listener
    let (auth_tx, mut auth_rx) = mpsc::channel(32);
    client.add_auth_state_change_listener(Box::new({
        let tx = auth_tx.clone();
        move |new_state: &DxLinkAuthState, _old: &DxLinkAuthState| {
            let new_state = *new_state;
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(new_state).await.unwrap();
            });
        }
    })).await;

    // Connect to server
    let con = client.connect(DEMO_DXLINK_WS_URL.to_string()).await;
    assert!(con.is_ok());
    tracing::debug!("Connected to server");

    // Wait for Connecting state
    let state = timeout(Duration::from_secs(5), conn_rx.recv())
        .await
        .expect("Timeout waiting for connecting state")
        .unwrap();
    assert_eq!(state, DxLinkConnectionState::Connecting);

    // Wait for Connected state
    let state = timeout(Duration::from_secs(5), conn_rx.recv())
        .await
        .expect("Timeout waiting for connected state")
        .unwrap();
    assert_eq!(state, DxLinkConnectionState::Connected);

    // Wait for Authorized state
    let state = timeout(Duration::from_secs(5), auth_rx.recv())
        .await
        .expect("Timeout waiting for authorized state")
        .unwrap();
    assert_eq!(state, DxLinkAuthState::Authorized);

    // Make sure we are in a good state before proceeding
    wait_for_states(&client).await;
    tracing::debug!("Client states verified, proceeding with channel open");

    // Set up state change and message channels
    let (state_tx, mut state_rx) = mpsc::channel(32);
    let (msg_tx, mut msg_rx) = mpsc::channel::<MessageType>(32);

    // Set up message listener first
    let msg_listener = Box::new({
        let tx = msg_tx.clone();
        move |msg: &DxLinkChannelMessage| {
            tracing::info!("Received channel message: {:?}", msg);
            let tx = tx.clone();
            let message = match msg.message_type.as_str() {
                "CHANNEL_OPENED" => {
                    tracing::info!("Processing CHANNEL_OPENED message");
                    let params = msg.payload.get("parameters")
                        .and_then(|v| v.as_object())
                        .map(|obj| {
                            obj.iter().map(|(k, v)| {
                                (k.clone(), v.clone())
                            }).collect::<HashMap<String, serde_json::Value>>()
                        });

                    Some(MessageType::ChannelOpened(ChannelOpenedMessage {
                        message_type: msg.message_type.clone(),
                        channel: msg.payload.get("channel")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        service: msg.payload.get("service")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        parameters: params
                    }))
                },
                "FEED_CONFIG" => {
                    tracing::info!("Processing FEED_CONFIG message");
                    serde_json::from_value(msg.payload.clone())
                        .map(|config| MessageType::FeedConfig(config))
                        .ok()
                },
                "FEED_DATA" => {
                    tracing::info!("Processing FEED_DATA message");
                    serde_json::from_value(msg.payload.clone())
                        .map(|data| MessageType::FeedData(data))
                        .ok()
                },
                "CHANNEL_CLOSED" => {
                    tracing::info!("Processing CHANNEL_CLOSED message");
                    Some(MessageType::ChannelClosed(ChannelClosedMessage {
                        message_type: msg.message_type.clone(),
                        channel: msg.payload.get("channel")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                    }))
                },
                _ => {
                    tracing::info!("Ignoring message type: {}", msg.message_type);
                    None
                }
            };
            
            if let Some(message_type) = message {
                tokio::spawn(async move {
                    tracing::info!("Sending message to channel: {:?}", message_type);
                    tx.send(message_type).await.unwrap();
                });
            }
        }
    });

    // Set up state change listener before creating the channel
    let state_listener = Box::new({
        let tx = state_tx.clone();
        move |new_state: &DxLinkChannelState, old_state: &DxLinkChannelState| {
            tracing::debug!("Channel state changed from {:?} to {:?}", old_state, new_state);
            let new_state = *new_state;
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(new_state).await.unwrap();
            });
        }
    });

    // Create the channel
    let channel = client
        .open_channel("FEED".to_string(), json!({"contract": "AUTO"}))
        .await;
    tracing::debug!("Channel created");

    // Add the listeners to the channel
    let _msg_listener_id = channel.add_message_listener(msg_listener);
    let _state_listener_id = channel.add_state_change_listener(state_listener);
    tracing::debug!("Listeners added");

    // Wait for the channel to be opened
    let mut channel_opened = false;
    let start_time = std::time::Instant::now();
    while !channel_opened {
        if start_time.elapsed() > Duration::from_secs(5) {
            panic!("Timeout waiting for channel to open. Current state: {:?}", channel.state());
        }
        
        match timeout(Duration::from_millis(100), state_rx.recv()).await {
            Ok(Some(state)) => {
                if state == DxLinkChannelState::Opened {
                    channel_opened = true;
                    tracing::debug!("Channel opened (from state change)");
                }
            }
            Ok(None) => {
                // Channel closed
                break;
            }
            Err(_) => {
                // Timeout, check current state
                if channel.state() == DxLinkChannelState::Opened {
                    channel_opened = true;
                    tracing::debug!("Channel opened (from direct state check)");
                }
            }
        }
    }

    assert!(channel_opened, "Channel failed to open within timeout period");

    // Add a small delay to ensure the channel is fully initialized
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Add a small delay to ensure message handling is complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Set up FEED service configuration
    tracing::info!("Sending feed setup message");
    let setup_msg = DxLinkChannelMessage {
        message_type: "FEED_SETUP".to_string(),
        payload: json!({
            "type": "FEED_SETUP",
            "channel": channel.id,
            "acceptAggregationPeriod": 10,
            "acceptDataFormat": "COMPACT",
            "acceptEventFields": {
                "Quote": ["eventType", "eventSymbol", "bidPrice", "askPrice", "bidSize", "askSize"]
            }
        })
    };
    tracing::debug!("FEED_SETUP message: {:?}", setup_msg);
    let setup_result = channel.send(setup_msg).await;
    tracing::debug!("FEED_SETUP result: {:?}", setup_result);
    assert!(setup_result.is_ok(), "Failed to send FEED_SETUP message: {:?}", setup_result);
    
    // Wait for FEED_CONFIG response
    tracing::info!("Waiting for FEED_CONFIG message");
    if let Ok(Some(msg)) = timeout(Duration::from_secs(5), msg_rx.recv()).await {
        match msg {
            MessageType::FeedConfig(config) => {
                tracing::info!("Received feed config: {:?}", config);
            },
            _ => panic!("Expected FEED_CONFIG message but got: {:?}", msg),
        }
    } else {
        tracing::warn!("Timeout waiting for FEED_CONFIG message");
    }

    // Send subscription request
    tracing::info!("Sending feed subscription message");
    let sub_msg = DxLinkChannelMessage {
        message_type: "FEED_SUBSCRIPTION".to_string(),
        payload: json!({
            "type": "FEED_SUBSCRIPTION",
            "channel": channel.id,
            "add": [{ "symbol": "AAPL", "type": "Quote" }]
        })
    };
    tracing::debug!("FEED_SUBSCRIPTION message: {:?}", sub_msg);
    let sub_result = channel.send(sub_msg).await;
    tracing::debug!("FEED_SUBSCRIPTION result: {:?}", sub_result);

    // Wait for feed data
    tracing::info!("Waiting for feed data");
    let mut received_data = false;
    while !received_data {
        let received_msg = timeout(Duration::from_secs(5), msg_rx.recv()).await;
        if let Ok(Some(msg)) = received_msg {
            match msg {
                MessageType::FeedConfig(config) => {
                    tracing::info!("Received feed config: {:?}", config);
                    // Continue waiting for FEED_DATA
                },
                MessageType::FeedData(data) => {
                    tracing::info!("Received feed data: {:?}", data);
                    // Verify it's a Quote event for AAPL
                    assert_eq!(data.channel, channel.id);
                    // Further data validation could be done here if needed
                    received_data = true;
                },
                _ => panic!("Expected FEED_CONFIG or FEED_DATA message but got: {:?}", msg),
            }
        } else {
            panic!("Timeout waiting for feed data");
        }
    }

    // Clean close the channel
    tracing::info!("Closing channel");
    let close_result = channel.close().await;
    assert!(close_result.is_ok(), "Failed to close channel: {:?}", close_result);

    // Wait for channel closed message
    tracing::info!("Waiting for CHANNEL_CLOSED message");
    if let Ok(Some(msg)) = timeout(Duration::from_secs(5), msg_rx.recv()).await {
        match msg {
            MessageType::ChannelClosed(closed) => {
                assert_eq!(closed.channel, channel.id);
            },
            _ => panic!("Expected CHANNEL_CLOSED message but got: {:?}", msg),
        }
    }

    // Disconnect client
    tracing::info!("Disconnecting client");
    let _ = client.disconnect().await;
}
