use std::time::Duration;

/// Log level for internal logging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DxLinkLogLevel {
    /// Debug level logging
    Debug,
    /// Info level logging
    Info,
    /// Warning level logging
    Warn,
    /// Error level logging
    Error,
}

/// Configuration options for the WebSocket client
#[derive(Debug, Clone)]
pub struct DxLinkWebSocketClientConfig {
    /// Interval in seconds between keepalive messages sent to server
    pub keepalive_interval: Duration,

    /// Timeout in seconds for server to detect that client is disconnected
    pub keepalive_timeout: Duration,

    /// Preferred timeout in seconds for client to detect that server is disconnected
    pub accept_keepalive_timeout: Duration,

    /// Timeout for action which requires update from server
    pub action_timeout: Duration,

    /// Log level for internal logger
    pub log_level: DxLinkLogLevel,

    /// Maximum number of reconnect attempts
    /// If connection is not established after this number of attempts, connection will be closed
    /// If 0, then reconnect attempts are not limited
    pub max_reconnect_attempts: u32,
}

impl Default for DxLinkWebSocketClientConfig {
    fn default() -> Self {
        Self {
            keepalive_interval: Duration::from_secs(30),
            keepalive_timeout: Duration::from_secs(60),
            accept_keepalive_timeout: Duration::from_secs(60),
            action_timeout: Duration::from_secs(30),
            log_level: DxLinkLogLevel::Info,
            max_reconnect_attempts: 0,
        }
    }
}

impl DxLinkWebSocketClientConfig {
    /// Creates a builder for DxLinkWebSocketClientConfig
    pub fn builder() -> DxLinkWebSocketClientConfigBuilder {
        DxLinkWebSocketClientConfigBuilder::default()
    }
}

/// Builder for DxLinkWebSocketClientConfig
#[derive(Default)]
pub struct DxLinkWebSocketClientConfigBuilder {
    keepalive_interval: Option<Duration>,
    keepalive_timeout: Option<Duration>,
    accept_keepalive_timeout: Option<Duration>,
    action_timeout: Option<Duration>,
    log_level: Option<DxLinkLogLevel>,
    max_reconnect_attempts: Option<u32>,
}

impl DxLinkWebSocketClientConfigBuilder {
    /// Sets the keepalive interval
    pub fn keepalive_interval(mut self, interval: Duration) -> Self {
        self.keepalive_interval = Some(interval);
        self
    }

    /// Sets the keepalive timeout
    pub fn keepalive_timeout(mut self, timeout: Duration) -> Self {
        self.keepalive_timeout = Some(timeout);
        self
    }

    /// Sets the accept keepalive timeout
    pub fn accept_keepalive_timeout(mut self, timeout: Duration) -> Self {
        self.accept_keepalive_timeout = Some(timeout);
        self
    }

    /// Sets the action timeout
    pub fn action_timeout(mut self, timeout: Duration) -> Self {
        self.action_timeout = Some(timeout);
        self
    }

    /// Sets the log level
    pub fn log_level(mut self, level: DxLinkLogLevel) -> Self {
        self.log_level = Some(level);
        self
    }

    /// Sets the maximum number of reconnect attempts
    pub fn max_reconnect_attempts(mut self, attempts: u32) -> Self {
        self.max_reconnect_attempts = Some(attempts);
        self
    }

    /// Builds the configuration
    pub fn build(self) -> DxLinkWebSocketClientConfig {
        let default = DxLinkWebSocketClientConfig::default();
        DxLinkWebSocketClientConfig {
            keepalive_interval: self
                .keepalive_interval
                .unwrap_or(default.keepalive_interval),
            keepalive_timeout: self.keepalive_timeout.unwrap_or(default.keepalive_timeout),
            accept_keepalive_timeout: self
                .accept_keepalive_timeout
                .unwrap_or(default.accept_keepalive_timeout),
            action_timeout: self.action_timeout.unwrap_or(default.action_timeout),
            log_level: self.log_level.unwrap_or(default.log_level),
            max_reconnect_attempts: self
                .max_reconnect_attempts
                .unwrap_or(default.max_reconnect_attempts),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DxLinkWebSocketClientConfig::default();
        assert_eq!(config.keepalive_interval, Duration::from_secs(30));
        assert_eq!(config.keepalive_timeout, Duration::from_secs(60));
        assert_eq!(config.accept_keepalive_timeout, Duration::from_secs(60));
        assert_eq!(config.action_timeout, Duration::from_secs(30));
        assert_eq!(config.log_level, DxLinkLogLevel::Info);
        assert_eq!(config.max_reconnect_attempts, 0);
    }

    #[test]
    fn test_builder() {
        let config = DxLinkWebSocketClientConfig::builder()
            .keepalive_interval(Duration::from_secs(15))
            .keepalive_timeout(Duration::from_secs(45))
            .accept_keepalive_timeout(Duration::from_secs(45))
            .action_timeout(Duration::from_secs(20))
            .log_level(DxLinkLogLevel::Debug)
            .max_reconnect_attempts(5)
            .build();

        assert_eq!(config.keepalive_interval, Duration::from_secs(15));
        assert_eq!(config.keepalive_timeout, Duration::from_secs(45));
        assert_eq!(config.accept_keepalive_timeout, Duration::from_secs(45));
        assert_eq!(config.action_timeout, Duration::from_secs(20));
        assert_eq!(config.log_level, DxLinkLogLevel::Debug);
        assert_eq!(config.max_reconnect_attempts, 5);
    }
}
