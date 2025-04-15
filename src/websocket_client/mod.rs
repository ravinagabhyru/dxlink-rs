pub mod config;
pub mod messages;
pub mod channel;
pub mod connector;
pub mod client;

#[cfg(test)]
mod tests;

pub use config::{DxLinkWebSocketClientConfig, DxLinkLogLevel};
pub use connector::WebSocketConnector;
pub use crate::DxLinkChannelMessage;

pub use channel::{
    DxLinkWebSocketChannel,
    // MessageListener,
    // StateChangeListener,
};
pub use messages::{
    AuthMessage, ChannelCancelMessage, ChannelClosedMessage,
    ChannelOpenedMessage, ChannelRequestMessage, ErrorMessage,
    KeepaliveMessage, Message, SetupMessage,
};
pub use client::DxLinkWebSocketClient;
