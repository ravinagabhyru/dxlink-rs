pub mod channel;
pub mod client;
pub mod config;
pub mod connector;
pub mod messages;

#[cfg(test)]
mod tests;

pub use crate::DxLinkChannelMessage;
pub use config::{DxLinkLogLevel, DxLinkWebSocketClientConfig};
pub use connector::WebSocketConnector;

pub use channel::{
    DxLinkWebSocketChannel,
    // MessageListener,
    // StateChangeListener,
};
pub use client::DxLinkWebSocketClient;
pub use messages::{
    AuthMessage, ChannelCancelMessage, ChannelClosedMessage, ChannelOpenedMessage,
    ChannelRequestMessage, ErrorMessage, KeepaliveMessage, Message, SetupMessage,
};
