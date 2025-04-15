use dxlink_rs::{
    DomService, BidAskEntry, DomSetupMessage, DomConfigMessage, DomSnapshotMessage,
};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_dom_service_lifecycle() {
    // Create channels for message passing
    let (tx, mut rx) = mpsc::channel(100);
    let service = DomService::new(tx);

    // Test initialization
    let channel = 1;
    service.initialize(channel, None).await.unwrap();

    // Verify setup message
    let msg = rx.recv().await.unwrap();
    let setup_msg = msg.as_any().downcast_ref::<DomSetupMessage>().unwrap();
    assert_eq!(setup_msg.channel, channel);
    assert_eq!(setup_msg.accept_aggregation_period, Some(1000));
    assert_eq!(setup_msg.accept_depth_limit, Some(10));
    assert_eq!(setup_msg.accept_data_format, Some("FULL".to_string()));

    // Test config handling
    let config = DomConfigMessage {
        message_type: "DOM_CONFIG".to_string(),
        channel,
        aggregation_period: 500,
        depth_limit: 20,
        data_format: "FULL".to_string(),
        order_fields: vec!["price".to_string(), "size".to_string()],
    };
    service.handle_config(config).await.unwrap();

    // Test snapshot handling
    let snapshot = DomSnapshotMessage {
        message_type: "DOM_SNAPSHOT".to_string(),
        channel,
        time: 1234567890,
        bids: vec![
            BidAskEntry { price: 100.0, size: 10.0 },
            BidAskEntry { price: 99.0, size: 20.0 },
        ],
        asks: vec![
            BidAskEntry { price: 101.0, size: 5.0 },
            BidAskEntry { price: 102.0, size: 15.0 },
        ],
    };
    service.handle_snapshot(snapshot).await.unwrap();

    // Verify final state
    let state = service.get_state().await;
    assert_eq!(state.channel, channel);
    assert_eq!(state.aggregation_period, 500);
    assert_eq!(state.depth_limit, 20);
    assert_eq!(state.bids.len(), 2);
    assert_eq!(state.asks.len(), 2);
    assert_eq!(state.bids[0].price, 100.0);
    assert_eq!(state.bids[1].price, 99.0);
    assert_eq!(state.asks[0].price, 101.0);
    assert_eq!(state.asks[1].price, 102.0);
}

#[tokio::test]
async fn test_dom_service_custom_config() {
    let (tx, mut rx) = mpsc::channel(100);
    let service = DomService::new(tx);

    // Test initialization with custom config
    let channel = 2;
    let custom_config = DomSetupMessage {
        message_type: "DOM_SETUP".to_string(),
        channel,
        accept_aggregation_period: Some(2000),
        accept_depth_limit: Some(5),
        accept_data_format: Some("COMPACT".to_string()),
        accept_order_fields: Some(vec!["price".to_string(), "size".to_string(), "orders".to_string()]),
    };
    service.initialize(channel, Some(custom_config)).await.unwrap();

    // Verify custom setup message
    let msg = rx.recv().await.unwrap();
    let setup_msg = msg.as_any().downcast_ref::<DomSetupMessage>().unwrap();
    assert_eq!(setup_msg.channel, channel);
    assert_eq!(setup_msg.accept_aggregation_period, Some(2000));
    assert_eq!(setup_msg.accept_depth_limit, Some(5));
    assert_eq!(setup_msg.accept_data_format, Some("COMPACT".to_string()));
    assert_eq!(setup_msg.accept_order_fields.as_ref().unwrap().len(), 3);
} 