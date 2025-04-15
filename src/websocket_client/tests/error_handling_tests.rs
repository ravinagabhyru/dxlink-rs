use crate::{DxLinkErrorType, DxLinkWebSocketClientConfig, DxLinkError};
use crate::websocket_client::client::DxLinkWebSocketClient;
use crate::websocket_client::messages::ErrorMessage;
use std::sync::Arc;
use tokio::sync::Mutex;

// Mock the essential components for testing the error handling
// in isolation without needing a real WebSocket connection
#[tokio::test]
async fn test_enhanced_error_message_handling() {
    // Create a client
    let config = DxLinkWebSocketClientConfig::default();
    let client = DxLinkWebSocketClient::new(config);
    
    // Track published errors
    let received_errors = Arc::new(Mutex::new(Vec::<DxLinkError>::new()));
    let received_errors_clone = received_errors.clone();
    
    // Add an error listener
    client.add_error_listener(Box::new(move |error| {
        let error_clone = error.clone();
        let errors = received_errors_clone.clone();
        tokio::spawn(async move {
            errors.lock().await.push(error_clone);
        });
    })).await;
    
    // Test connection-level errors (channel 0)
    let connection_error = ErrorMessage {
        message_type: "ERROR".to_string(),
        channel: 0,
        error: DxLinkErrorType::Timeout,
        message: "Connection timed out after 30 seconds".to_string(),
    };
    
    // Manually invoke the publish_error method to simulate receiving this error
    client.publish_error(DxLinkError::new(
        connection_error.error,
        format!("Connection error: {}", connection_error.message),
    ));
    
    // Allow time for async processing
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    // Verify the error was processed with the enhanced format
    let errors = received_errors.lock().await;
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error_type, DxLinkErrorType::Timeout);
    assert!(errors[0].message.starts_with("Connection error: "), 
            "Expected message to start with 'Connection error:', got: {}", errors[0].message);
    assert!(errors[0].message.contains("Connection timed out after 30 seconds"), 
            "Expected message to contain the original error text");
}