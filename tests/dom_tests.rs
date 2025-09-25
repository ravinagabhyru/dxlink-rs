use dxlink_rs::{
    BidAskEntry, DomConfigMessage, DomService, DomSetupMessage, DomSnapshotMessage, DxLinkErrorType,
};
use std::{env, sync::Once};
use tokio::sync::mpsc;
use tracing::{debug, info, trace, Level};

static INIT_TRACING: Once = Once::new();

fn init_tracing() {
    INIT_TRACING.call_once(|| {
        let level = env::var("DOM_TEST_LOG")
            .or_else(|_| env::var("RUST_LOG"))
            .ok()
            .and_then(|val| val.parse::<Level>().ok())
            .unwrap_or(Level::TRACE);

        let _ = tracing_subscriber::fmt().with_max_level(level).try_init();
    });
}

#[tokio::test]
async fn test_dom_service_lifecycle() {
    init_tracing();
    info!("starting dom_service_lifecycle test");
    // Create channels for message passing
    let (tx, mut rx) = mpsc::channel(100);
    let service = DomService::new(tx);

    // Test initialization
    let channel = 1;
    info!(
        channel = channel,
        "initializing DOM service with default configuration"
    );
    service.initialize(channel, None).await.unwrap();

    // Verify setup message
    let msg = rx.recv().await.unwrap();
    let setup_msg = msg.as_any().downcast_ref::<DomSetupMessage>().unwrap();
    debug!(?setup_msg, "received DOM setup message");
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
    info!(channel = channel, "sending DOM_CONFIG message");
    service.handle_config(config).await.unwrap();

    // Test snapshot handling
    let snapshot = DomSnapshotMessage {
        message_type: "DOM_SNAPSHOT".to_string(),
        channel,
        time: 1234567890,
        sequence: Some(7),
        bids: vec![
            BidAskEntry {
                price: 100.0,
                size: 10.0,
            },
            BidAskEntry {
                price: 99.0,
                size: 20.0,
            },
        ],
        asks: vec![
            BidAskEntry {
                price: 101.0,
                size: 5.0,
            },
            BidAskEntry {
                price: 102.0,
                size: 15.0,
            },
        ],
    };
    trace!(?snapshot, "sending DOM_SNAPSHOT update");
    service.handle_snapshot(snapshot).await.unwrap();

    // Verify final state
    let state = service.get_state().await;
    debug!(state = ?state, "final DOM state after lifecycle");
    assert_eq!(state.channel, channel);
    assert_eq!(state.aggregation_period, 500);
    assert_eq!(state.depth_limit, 20);
    assert_eq!(state.data_format, "FULL");
    assert_eq!(
        state.order_fields,
        vec!["price".to_string(), "size".to_string()]
    );
    assert_eq!(state.bids.len(), 2);
    assert_eq!(state.asks.len(), 2);
    assert_eq!(state.bids[0].price, 100.0);
    assert_eq!(state.bids[1].price, 99.0);
    assert_eq!(state.asks[0].price, 101.0);
    assert_eq!(state.asks[1].price, 102.0);
    assert_eq!(state.last_update_time, 1234567890);
    assert_eq!(state.sequence, Some(7));
}

#[tokio::test]
async fn test_dom_service_custom_config() {
    init_tracing();
    info!("starting dom_service_custom_config test");
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
        accept_order_fields: Some(vec![
            "price".to_string(),
            "size".to_string(),
            "orders".to_string(),
        ]),
    };
    info!(
        channel = channel,
        "initializing DOM service with custom configuration"
    );
    service
        .initialize(channel, Some(custom_config))
        .await
        .unwrap();

    // Verify custom setup message
    let msg = rx.recv().await.unwrap();
    let setup_msg = msg.as_any().downcast_ref::<DomSetupMessage>().unwrap();
    debug!(setup = ?setup_msg, "received custom DOM setup message");
    assert_eq!(setup_msg.channel, channel);
    assert_eq!(setup_msg.accept_aggregation_period, Some(2000));
    assert_eq!(setup_msg.accept_depth_limit, Some(5));
    assert_eq!(setup_msg.accept_data_format, Some("COMPACT".to_string()));
    assert_eq!(setup_msg.accept_order_fields.as_ref().unwrap().len(), 3);

    let state = service.get_state().await;
    debug!(state = ?state, "state after custom config initialization");
    assert_eq!(state.channel, channel);
    assert_eq!(state.aggregation_period, 2000);
    assert_eq!(state.depth_limit, 5);
    assert_eq!(state.data_format, "COMPACT");
    assert_eq!(
        state.order_fields,
        vec!["price", "size", "orders"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_dom_service_depth_limit_and_sorting() {
    init_tracing();
    info!("starting dom_service_depth_limit_and_sorting test");
    let (tx, _rx) = mpsc::channel(16);
    let service = DomService::new(tx);

    let channel = 10;
    info!(channel = channel, "initializing DOM service on channel");
    service.initialize(channel, None).await.unwrap();

    let config = DomConfigMessage {
        message_type: "DOM_CONFIG".to_string(),
        channel,
        aggregation_period: 100,
        depth_limit: 2,
        data_format: "FULL".to_string(),
        order_fields: vec!["price".to_string(), "size".to_string()],
    };
    trace!(config = ?config, "applying depth-limited DOM config");
    service.handle_config(config).await.unwrap();

    let snapshot = DomSnapshotMessage {
        message_type: "DOM_SNAPSHOT".to_string(),
        channel,
        time: 777,
        sequence: Some(5),
        bids: vec![
            BidAskEntry {
                price: 101.0,
                size: 5.0,
            },
            BidAskEntry {
                price: 103.0,
                size: 1.0,
            },
            BidAskEntry {
                price: 102.0,
                size: 2.0,
            },
        ],
        asks: vec![
            BidAskEntry {
                price: 100.5,
                size: 3.0,
            },
            BidAskEntry {
                price: 100.2,
                size: 4.0,
            },
            BidAskEntry {
                price: 100.1,
                size: 1.0,
            },
        ],
    };

    trace!(snapshot = ?snapshot, "sending depth-limited snapshot");
    service.handle_snapshot(snapshot).await.unwrap();
    let state = service.get_state().await;
    debug!(state = ?state, "state after depth limit enforcement");
    assert_eq!(state.bids.len(), 2);
    assert_eq!(state.bids[0].price, 103.0);
    assert_eq!(state.bids[1].price, 102.0);
    assert_eq!(state.asks.len(), 2);
    assert_eq!(state.asks[0].price, 100.1);
    assert_eq!(state.asks[1].price, 100.2);
    assert_eq!(state.last_update_time, 777);
    assert_eq!(state.sequence, Some(5));
}

#[tokio::test]
async fn test_dom_service_rejects_invalid_snapshot() {
    init_tracing();
    info!("starting dom_service_rejects_invalid_snapshot test");
    let (tx, _rx) = mpsc::channel(4);
    let service = DomService::new(tx);
    service.initialize(3, None).await.unwrap();

    let bad_snapshot = DomSnapshotMessage {
        message_type: "DOM_SNAPSHOT".to_string(),
        channel: 3,
        time: 1,
        sequence: None,
        bids: vec![BidAskEntry {
            price: f64::NAN,
            size: 1.0,
        }],
        asks: vec![BidAskEntry {
            price: 100.0,
            size: -1.0,
        }],
    };

    let err = service.handle_snapshot(bad_snapshot).await.unwrap_err();
    assert_eq!(err.error_type, DxLinkErrorType::InvalidMessage);
}

#[tokio::test]
async fn test_dom_service_rejects_zero_channel_initialization() {
    init_tracing();
    info!("starting dom_service_rejects_zero_channel_initialization test");
    let (tx, _rx) = mpsc::channel(4);
    let service = DomService::new(tx);

    let err = service.initialize(0, None).await.unwrap_err();
    assert_eq!(err.error_type, DxLinkErrorType::BadAction);
}
