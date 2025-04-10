use std::fmt;
use crate::core::errors::DxLinkError;

/// Message that can be sent to or received from the channel.
#[derive(Debug, Clone)]
pub struct DxLinkChannelMessage {
    /// Type of the message
    pub message_type: String,
    /// Payload of the message as key-value pairs
    pub payload: serde_json::Value,
}

/// Channel state that can be used to check if channel is available for sending messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DxLinkChannelState {
    /// Channel is requested and cannot be used to send messages yet.
    Requested,
    /// Channel is opened and can be used to send messages.
    Opened,
    /// Channel was closed by close() or by server.
    /// Channel cannot be used anymore.
    Closed,
}

impl fmt::Display for DxLinkChannelState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DxLinkChannelState::Requested => write!(f, "REQUESTED"),
            DxLinkChannelState::Opened => write!(f, "OPENED"),
            DxLinkChannelState::Closed => write!(f, "CLOSED"),
        }
    }
}

/// Callback type for messages from the channel.
pub type ChannelMessageListener = Box<dyn Fn(&DxLinkChannelMessage) + Send + Sync>;

/// Callback type for channel state changes.
pub type ChannelStateChangeListener = Box<dyn Fn(&DxLinkChannelState, &DxLinkChannelState) + Send + Sync>;

/// Callback type for errors from the server.
pub type ChannelErrorListener = Box<dyn Fn(&DxLinkError) + Send + Sync>;

/// Isolated channel to service within single WebSocket connection to remote endpoint.
// pub trait DxLinkChannel {
//     /// Clone this channel into a boxed trait object
//     fn clone_box(&self) -> Box<dyn DxLinkChannel + Send + Sync>;
//     /// Get the unique identifier of the channel.
//     fn id(&self) -> u64;

//     /// Get the name of the service that channel is opened to.
//     fn service(&self) -> &str;

//     /// Get the parameters of the service that channel is opened to.
//     fn parameters(&self) -> HashMap<String, serde_json::Value>;

//     /// Send a message to the channel.
//     fn send(&self, message: DxLinkChannelMessage);

//     /// Add a listener for messages from the channel.
//     fn add_message_listener(&self, listener: ChannelMessageListener);

//     /// Remove a listener for messages from the channel.
//     fn remove_message_listener(&mut self, listener: ChannelMessageListener);

//     /// Get channel state that can be used to check if channel is available for sending messages.
//     fn state(&self) -> DxLinkChannelState;

//     /// Add a listener for channel state changes.
//     /// If channel is ready to use, listener will be called immediately with DxLinkChannelState::Opened state.
//     /// Note: when remote endpoint reconnects, channel will be reopened and listener will be called with
//     /// DxLinkChannelState::Opened state again.
//     fn add_state_change_listener(&mut self, listener: ChannelStateChangeListener);

//     /// Remove a listener for channel state changes.
//     fn remove_state_change_listener(&mut self, listener: ChannelStateChangeListener);

//     /// Add a listener for errors from the server.
//     fn add_error_listener(&mut self, listener: ChannelErrorListener);

//     /// Remove a listener for errors from the server.
//     fn remove_error_listener(&mut self, listener: ChannelErrorListener);

//     /// Close the channel and free all resources.
//     /// This method does nothing if the channel is already closed.
//     /// The channel state immediately becomes Closed.
//     fn close(&mut self);
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_state_display() {
        assert_eq!(DxLinkChannelState::Requested.to_string(), "REQUESTED");
        assert_eq!(DxLinkChannelState::Opened.to_string(), "OPENED");
        assert_eq!(DxLinkChannelState::Closed.to_string(), "CLOSED");
    }

    #[test]
    fn test_channel_state_equality() {
        assert_eq!(DxLinkChannelState::Requested, DxLinkChannelState::Requested);
        assert_ne!(DxLinkChannelState::Opened, DxLinkChannelState::Closed);
    }
}
