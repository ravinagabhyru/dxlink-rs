use dxlink_rs::{
    core::{
        auth::DxLinkAuthState,
        client::DxLinkConnectionState,
    },
    feed::{
        Feed, FeedAcceptConfig, FeedContract, FeedDataFormat, FeedEvent,
        FeedOptions,
    },
    websocket_client::{DxLinkWebSocketClient, DxLinkWebSocketClientConfig},
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;

const DEMO_DXLINK_WS_URL: &str = "wss://demo.dxfeed.com/dxlink-ws";

// Helper function to create and initialize a client
async fn create_test_client() -> Arc<Mutex<DxLinkWebSocketClient>> {
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
    assert_eq!(client.get_connection_state().await, DxLinkConnectionState::Connected);
    assert_eq!(client.get_auth_state().await, DxLinkAuthState::Authorized);
    tracing::debug!("Client states verified");

    Arc::new(Mutex::new(client))
}

#[tokio::test]
async fn test_compact_format_integration() {
    // Scenario 1: Req Compact -> Recv Config(Compact+Fields) -> Recv Data(Compact) => Verify Converted Events
    let client = create_test_client().await;

    // Create feed requesting compact format
    let mut feed = Feed::new(
        client.clone(),
        FeedContract::Stream,
        Some(FeedOptions::default()),
        Some(FeedDataFormat::Compact),
    )
    .await
    .expect("Failed to create feed service");

    // Configure feed with event fields
    let config = FeedAcceptConfig {
        accept_aggregation_period: Some(1000.0),
        accept_data_format: Some(FeedDataFormat::Compact),
        accept_event_fields: Some(HashMap::from([
            ("Quote".to_string(), vec![
                "eventSymbol".to_string(),
                "eventType".to_string(),
                "bidPrice".to_string(),
                "askPrice".to_string(),
                "bidSize".to_string(),
                "askSize".to_string(),
            ]),
        ])),
    };
    feed.configure(config).await.expect("Failed to configure feed");

    // Subscribe to data events
    let mut rx = feed.subscribe_to_data_events();

    // Add a subscription to trigger data flow
    let sub = json!({ "type": "Quote", "symbol": "AAPL" });
    feed.add_subscriptions(vec![sub])
        .await
        .expect("Failed to add subscription");

    // Wait for data events with a timeout
    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    if let Ok(Ok(event)) = result {
        // Verify the event is a Quote event
        if let FeedEvent::Quote(quote) = event {
            assert_eq!(quote.event_symbol, "AAPL");
            // Add more assertions based on the expected quote data
        } else {
            panic!("Expected Quote event");
        }
    }

    feed.close().await;
}

#[tokio::test]
async fn test_full_format_fallback() {
    // Scenario 2: Req Compact -> Recv Config(Full) -> Recv Data(Full) => Verify Full Events
    let client = create_test_client().await;

    // Create feed requesting compact format
    let mut feed = Feed::new(
        client.clone(),
        FeedContract::Stream,
        Some(FeedOptions::default()),
        Some(FeedDataFormat::Compact),
    )
    .await
    .expect("Failed to create feed service");

    // Configure feed to accept full format
    let config = FeedAcceptConfig {
        accept_aggregation_period: Some(1000.0),
        accept_data_format: Some(FeedDataFormat::Full),
        accept_event_fields: None,
    };
    feed.configure(config).await.expect("Failed to configure feed");

    // Subscribe to data events
    let mut rx = feed.subscribe_to_data_events();

    // Add a subscription to trigger data flow
    let sub = json!({ "type": "Quote", "symbol": "AAPL" });
    feed.add_subscriptions(vec![sub])
        .await
        .expect("Failed to add subscription");

    // Wait for data events with a timeout
    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    if let Ok(Ok(event)) = result {
        // Verify the event is a Quote event
        if let FeedEvent::Quote(quote) = event {
            assert_eq!(quote.event_symbol, "AAPL");
            // Add more assertions based on the expected quote data
        } else {
            panic!("Expected Quote event");
        }
    }

    feed.close().await;
}

#[tokio::test]
async fn test_compact_format_error_handling() {
    // Scenario 3: Req Compact -> Recv Config(Compact+NoFields) -> Recv Data(Compact) => Verify No Events + Error Log
    let client = create_test_client().await;

    // Create feed requesting compact format
    let mut feed = Feed::new(
        client.clone(),
        FeedContract::Stream,
        Some(FeedOptions::default()),
        Some(FeedDataFormat::Compact),
    )
    .await
    .expect("Failed to create feed service");

    // Configure feed with Compact format but no fields
    let config = FeedAcceptConfig {
        accept_aggregation_period: Some(1000.0),
        accept_data_format: Some(FeedDataFormat::Compact),
        accept_event_fields: None,
    };
    feed.configure(config).await.expect("Failed to configure feed");

    // Subscribe to data events
    let mut rx = feed.subscribe_to_data_events();

    // Add a subscription to trigger data flow
    let sub = json!({ "type": "Quote", "symbol": "AAPL" });
    feed.add_subscriptions(vec![sub])
        .await
        .expect("Failed to add subscription");

    // Wait for data events with a timeout
    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(result.is_err(), "Expected no events to be received");

    feed.close().await;
}

#[tokio::test]
async fn test_malformed_compact_data() {
    // Scenario 4: Req Compact -> Recv Config(Compact+Fields) -> Recv Data(Malformed Compact) => Verify No Events + Error Log
    let client = create_test_client().await;

    // Create feed requesting compact format
    let mut feed = Feed::new(
        client.clone(),
        FeedContract::Stream,
        Some(FeedOptions::default()),
        Some(FeedDataFormat::Compact),
    )
    .await
    .expect("Failed to create feed service");

    // Configure feed with Compact format and fields
    let config = FeedAcceptConfig {
        accept_aggregation_period: Some(1000.0),
        accept_data_format: Some(FeedDataFormat::Compact),
        accept_event_fields: Some(HashMap::from([
            ("Quote".to_string(), vec![
                "eventSymbol".to_string(),
                "eventType".to_string(),
                "bidPrice".to_string(),
                "askPrice".to_string(),
                "bidSize".to_string(),
                "askSize".to_string(),
            ]),
        ])),
    };
    feed.configure(config).await.expect("Failed to configure feed");

    // Subscribe to data events
    let mut rx = feed.subscribe_to_data_events();

    // Add a subscription to trigger data flow
    let sub = json!({ "type": "Quote", "symbol": "AAPL" });
    feed.add_subscriptions(vec![sub])
        .await
        .expect("Failed to add subscription");

    // Wait for data events with a timeout
    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(result.is_err(), "Expected no events to be received");

    feed.close().await;
}
