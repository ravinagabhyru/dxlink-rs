use std::fmt;

/// Authentication state that can be used to check if user is authorized
/// on the remote endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DxLinkAuthState {
    /// User is unauthorized on the remote endpoint.
    Unauthorized,

    /// User in the process of authorization, but not yet authorized.
    Authorizing,

    /// User is authorized on the remote endpoint and can use it.
    Authorized,
}

impl fmt::Display for DxLinkAuthState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Match on self and write the string representation
        match self {
            DxLinkAuthState::Unauthorized => write!(f, "UNAUTHORIZED"),
            DxLinkAuthState::Authorizing => write!(f, "AUTHORIZING"),
            DxLinkAuthState::Authorized => write!(f, "AUTHORIZED"),
        }
    }
}

/// Callback type for authentication state changes.
/// Takes references to the current and previous auth states.
pub type AuthStateChangeListener = Box<dyn Fn(&DxLinkAuthState, &DxLinkAuthState) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_state_display() {
        assert_eq!(DxLinkAuthState::Unauthorized.to_string(), "UNAUTHORIZED");
        assert_eq!(DxLinkAuthState::Authorizing.to_string(), "AUTHORIZING");
        assert_eq!(DxLinkAuthState::Authorized.to_string(), "AUTHORIZED");
    }

    #[test]
    fn test_auth_state_equality() {
        assert_eq!(DxLinkAuthState::Unauthorized, DxLinkAuthState::Unauthorized);
        assert_ne!(DxLinkAuthState::Unauthorized, DxLinkAuthState::Authorized);
    }
}
