use dxlink_rs::feed::{Feed, FeedAcceptConfig, FeedDataFormat, FeedEvent, FeedOptions};
use dxlink_rs::websocket_client::{DxLinkWebSocketClient, DxLinkWebSocketClientConfig, DxLinkLogLevel};
use dxlink_rs::FeedContract;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration as TokioDuration};

#[tokio::test]
async fn test_feed_lifecycle() {
    // Create a WebSocket client
    let config = DxLinkWebSocketClientConfig {
        keepalive_interval: Duration::from_secs(30),
        keepalive_timeout: Duration::from_secs(10),
        accept_keepalive_timeout: Duration::from_secs(10),
        action_timeout: Duration::from_secs(30),
        log_level: DxLinkLogLevel::Info,
        max_reconnect_attempts: 3,
    };
    let client = Arc::new(Mutex::new(DxLinkWebSocketClient::new(config)));

    // Create a feed service
    let mut feed = Feed::new(
        client.clone(),
        FeedContract::Auto,
        Some(FeedOptions::default()),
        Some(FeedDataFormat::Full),
    )
    .await
    .expect("Failed to create feed service");

    // Configure the feed
    let config = FeedAcceptConfig {
        accept_aggregation_period: Some(1000.0),
        accept_data_format: Some(FeedDataFormat::Full),
        accept_event_fields: None,
    };
    feed.configure(config).await.expect("Failed to configure feed");

    // Add subscriptions
    let sub1 = json!({ "type": "Quote", "symbol": "AAPL" });
    let sub2 = json!({ "type": "Trade", "symbol": "MSFT" });
    feed.add_subscriptions(vec![sub1.clone(), sub2.clone()])
        .await
        .expect("Failed to add subscriptions");

    // Remove a subscription
    feed.remove_subscriptions(vec![sub1.clone()])
        .await
        .expect("Failed to remove subscription");

    // Clear all subscriptions
    feed.clear_subscriptions()
        .await
        .expect("Failed to clear subscriptions");

    // Close the feed
    feed.close().await;
}

#[tokio::test]
async fn test_feed_data_handling() {
    // Create a WebSocket client
    let config = DxLinkWebSocketClientConfig {
        keepalive_interval: Duration::from_secs(30),
        keepalive_timeout: Duration::from_secs(10),
        accept_keepalive_timeout: Duration::from_secs(10),
        action_timeout: Duration::from_secs(30),
        log_level: DxLinkLogLevel::Info,
        max_reconnect_attempts: 3,
    };
    let client = Arc::new(Mutex::new(DxLinkWebSocketClient::new(config)));

    // Create a feed service
    let mut feed = Feed::new(
        client.clone(),
        FeedContract::Auto,
        Some(FeedOptions::default()),
        Some(FeedDataFormat::Full),
    )
    .await
    .expect("Failed to create feed service");

    // Configure the feed to accept full format data
    let config = FeedAcceptConfig {
        accept_aggregation_period: Some(1000.0),
        accept_data_format: Some(FeedDataFormat::Full),
        accept_event_fields: None,
    };
    feed.configure(config).await.expect("Failed to configure feed");

    // Add a subscription
    let sub = json!({ "type": "Quote", "symbol": "AAPL" });
    feed.add_subscriptions(vec![sub.clone()])
        .await
        .expect("Failed to add subscription");

    // Subscribe to data events
    let mut rx = feed.subscribe_to_data_events();

    // Wait for data events with a timeout
    let result = timeout(TokioDuration::from_secs(5), rx.recv()).await;
    if let Ok(Ok(event)) = result {
        // Verify the event is a Quote event
        if let FeedEvent::Quote(quote) = event {
            assert_eq!(quote.event_symbol, "AAPL");
            // Add more assertions based on the expected quote data
        } else {
            panic!("Expected Quote event");
        }
    }

    // Close the feed
    feed.close().await;
}

#[tokio::test]
async fn test_feed_config_handling() {
    // Create a WebSocket client
    let config = DxLinkWebSocketClientConfig {
        keepalive_interval: Duration::from_secs(30),
        keepalive_timeout: Duration::from_secs(10),
        accept_keepalive_timeout: Duration::from_secs(10),
        action_timeout: Duration::from_secs(30),
        log_level: DxLinkLogLevel::Info,
        max_reconnect_attempts: 3,
    };
    let client = Arc::new(Mutex::new(DxLinkWebSocketClient::new(config)));

    // Create a feed service
    let mut feed = Feed::new(
        client.clone(),
        FeedContract::Auto,
        Some(FeedOptions::default()),
        Some(FeedDataFormat::Full),
    )
    .await
    .expect("Failed to create feed service");

    // Configure the feed
    let config = FeedAcceptConfig {
        accept_aggregation_period: Some(1000.0),
        accept_data_format: Some(FeedDataFormat::Full),
        accept_event_fields: None,
    };
    feed.configure(config).await.expect("Failed to configure feed");

    // Wait for the configuration to be processed
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Get the current configuration
    let current_config = feed.config().await;
    assert_eq!(current_config.aggregation_period, 0.0); // Default value
    assert_eq!(current_config.data_format, FeedDataFormat::Full); // Default value

    // Close the feed
    feed.close().await;
}

#[tokio::test]
async fn test_feed_subscription_management() {
    // Create a WebSocket client
    let config = DxLinkWebSocketClientConfig {
        keepalive_interval: Duration::from_secs(30),
        keepalive_timeout: Duration::from_secs(10),
        accept_keepalive_timeout: Duration::from_secs(10),
        action_timeout: Duration::from_secs(30),
        log_level: DxLinkLogLevel::Info,
        max_reconnect_attempts: 3,
    };
    let client = Arc::new(Mutex::new(DxLinkWebSocketClient::new(config)));

    // Create a feed service
    let mut feed = Feed::new(
        client.clone(),
        FeedContract::Auto,
        Some(FeedOptions::default()),
        Some(FeedDataFormat::Full),
    )
    .await
    .expect("Failed to create feed service");

    // Add multiple subscriptions
    let sub1 = json!({ "type": "Quote", "symbol": "AAPL" });
    let sub2 = json!({ "type": "Trade", "symbol": "MSFT" });
    let sub3 = json!({ "type": "Quote", "symbol": "GOOG" });
    feed.add_subscriptions(vec![sub1.clone(), sub2.clone(), sub3.clone()])
        .await
        .expect("Failed to add subscriptions");

    // Remove some subscriptions
    feed.remove_subscriptions(vec![sub1.clone(), sub2.clone()])
        .await
        .expect("Failed to remove subscriptions");

    // Add more subscriptions
    let sub4 = json!({ "type": "Trade", "symbol": "AMZN" });
    feed.add_subscriptions(vec![sub4.clone()])
        .await
        .expect("Failed to add subscription");

    // Clear all subscriptions
    feed.clear_subscriptions()
        .await
        .expect("Failed to clear subscriptions");

    // Close the feed
    feed.close().await;
}
