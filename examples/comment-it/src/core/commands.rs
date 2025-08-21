use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Unified commands for the Kaspa authentication and comment episode
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum UnifiedCommand {
    // Auth commands
    /// Request a challenge from the server
    RequestChallenge,
    /// Submit response with signature and nonce
    SubmitResponse { signature: String, nonce: String },
    /// Revoke an existing session
    RevokeSession { session_token: String, signature: String },

    // Comment commands (only work after authentication)
    /// Submit a new comment to the blockchain
    SubmitComment { text: String, session_token: String },
}

impl UnifiedCommand {
    /// Get the command type as a string for logging/debugging
    pub fn command_type(&self) -> &'static str {
        match self {
            UnifiedCommand::RequestChallenge => "RequestChallenge",
            UnifiedCommand::SubmitResponse { .. } => "SubmitResponse",
            UnifiedCommand::RevokeSession { .. } => "RevokeSession",
            UnifiedCommand::SubmitComment { .. } => "SubmitComment",
        }
    }

    /// Check if command requires authentication
    pub fn requires_auth(&self) -> bool {
        match self {
            UnifiedCommand::RequestChallenge => false,
            UnifiedCommand::SubmitResponse { .. } => true,
            UnifiedCommand::RevokeSession { .. } => true,
            UnifiedCommand::SubmitComment { .. } => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_challenge_command() {
        let cmd = UnifiedCommand::RequestChallenge;
        assert_eq!(cmd.command_type(), "RequestChallenge");
        assert!(!cmd.requires_auth());
    }

    #[test]
    fn test_submit_response_command() {
        let cmd = UnifiedCommand::SubmitResponse { signature: "test_signature".to_string(), nonce: "test_nonce".to_string() };
        assert_eq!(cmd.command_type(), "SubmitResponse");
        assert!(cmd.requires_auth());
    }

    #[test]
    fn test_submit_comment_command() {
        let cmd = UnifiedCommand::SubmitComment { text: "Hello blockchain!".to_string(), session_token: "sess_123".to_string() };
        assert_eq!(cmd.command_type(), "SubmitComment");
        assert!(cmd.requires_auth());
    }

    #[test]
    fn test_serialization() {
        let cmd = UnifiedCommand::SubmitResponse { signature: "sig123".to_string(), nonce: "nonce456".to_string() };

        // Test that we can serialize and deserialize
        let serialized = serde_json::to_string(&cmd).unwrap();
        let deserialized: UnifiedCommand = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            UnifiedCommand::SubmitResponse { signature, nonce } => {
                assert_eq!(signature, "sig123");
                assert_eq!(nonce, "nonce456");
            }
            _ => panic!("Expected SubmitResponse"),
        }
    }
}
