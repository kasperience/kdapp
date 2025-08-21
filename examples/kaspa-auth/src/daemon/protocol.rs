// src/daemon/protocol.rs - IPC protocol between participant peer and daemon

use serde::{Deserialize, Serialize};

/// Session information for listing active sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub episode_id: kaspa_hashes::Hash,
    pub username: String,
    pub peer_url: String,
    pub session_token: String,
    pub created_at_seconds: u64,
}

/// Request sent from participant peer to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonRequest {
    /// Check if daemon is running and responsive
    Ping,

    /// Unlock the authentication identity with password/PIN
    Unlock { password: String, username: String },

    /// Lock the authentication identity (clear from memory)
    Lock,

    /// Get current authentication status
    Status,

    /// Sign authentication challenge
    SignChallenge { challenge: String, username: String },

    /// Create new authentication identity
    CreateIdentity { username: String, password: String },

    /// List available authentication identities
    ListIdentities,

    /// List active sessions
    ListSessions,

    /// Perform full authentication flow
    Authenticate { peer_url: String, username: String },

    /// Revoke active session
    RevokeSession { episode_id: u64, session_token: String, username: String },

    /// Shutdown daemon gracefully
    Shutdown,
}

/// Response sent from daemon to participant peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    /// Simple success/failure response
    Success { message: String },

    /// Error response with details
    Error { error: String },

    /// Ping response with daemon info
    Pong { version: String, uptime_seconds: u64, identities_loaded: usize },

    /// Authentication status response
    Status { is_unlocked: bool, loaded_identities: Vec<String>, active_sessions: usize },

    /// Signature response
    Signature { signature: String, public_key: String },

    /// Authentication result
    AuthResult { success: bool, episode_id: Option<u64>, session_token: Option<String>, message: String },

    /// List of identities
    Identities { usernames: Vec<String> },

    /// List of active sessions
    Sessions { sessions: Vec<SessionInfo> },
}

/// IPC message wrapper with error handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage<T> {
    pub id: u64,
    pub payload: T,
}

impl<T> IpcMessage<T> {
    pub fn new(payload: T) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);

        Self { id: COUNTER.fetch_add(1, Ordering::SeqCst), payload }
    }
}

/// Serialize message for IPC transport
pub fn serialize_message<T: Serialize>(msg: &IpcMessage<T>) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let json = serde_json::to_string(msg)?;
    let len = json.len() as u32;

    // Protocol: 4 bytes length + JSON payload
    let mut buffer = Vec::with_capacity(4 + json.len());
    buffer.extend_from_slice(&len.to_le_bytes());
    buffer.extend_from_slice(json.as_bytes());

    Ok(buffer)
}

/// Deserialize message from IPC transport
pub fn deserialize_message<T: for<'de> Deserialize<'de>>(buffer: &[u8]) -> Result<IpcMessage<T>, Box<dyn std::error::Error>> {
    if buffer.len() < 4 {
        return Err("Buffer too short for length header".into());
    }

    let len = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;

    if buffer.len() < 4 + len {
        return Err("Buffer too short for payload".into());
    }

    let json_bytes = &buffer[4..4 + len];
    let json = String::from_utf8(json_bytes.to_vec())?;
    let message = serde_json::from_str(&json)?;

    Ok(message)
}
