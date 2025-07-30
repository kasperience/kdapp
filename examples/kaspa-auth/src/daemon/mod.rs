// src/daemon/mod.rs - kaspa-auth-daemon: Background authentication service

pub mod service;
pub mod protocol;

pub use service::AuthDaemon;
pub use protocol::{DaemonRequest, DaemonResponse};

/// Configuration for the kaspa-auth daemon
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Directory to store wallet and other data
    pub data_dir: String,
    /// Socket path for IPC communication
    pub socket_path: String,
    /// Auto-unlock wallet on startup
    pub auto_unlock: bool,
    /// Session timeout in seconds
    pub session_timeout: u64,
    /// Enable keychain integration
    pub use_keychain: bool,
    /// Development mode (insecure storage)
    pub dev_mode: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            data_dir: ".".to_string(),
            socket_path: "/tmp/kaspa-auth.sock".to_string(),
            auto_unlock: false,
            session_timeout: 3600, // 1 hour
            use_keychain: true,
            dev_mode: false,
        }
    }
}

impl DaemonConfig {
    /// Create config for user session (uses user runtime directory)
    pub fn for_user_session() -> Self {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/tmp/kaspa-auth-{}", std::process::id()));
        
        Self {
            data_dir: ".".to_string(),
            socket_path: format!("{}/kaspa-auth.sock", runtime_dir),
            auto_unlock: true,
            session_timeout: 3600,
            use_keychain: true,
            dev_mode: false,
        }
    }
}