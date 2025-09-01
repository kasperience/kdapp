// src/cli/commands/daemon.rs - CLI commands for kaspa-auth-daemon

use clap::Args;
use std::error::Error;
#[cfg(windows)]
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;

// Platform-specific type alias
#[cfg(unix)]
type PlatformStream = UnixStream;
#[cfg(windows)]
type PlatformStream = TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::daemon::{protocol::*, AuthDaemon, DaemonConfig};

#[derive(Args)]
pub struct DaemonCommand {
    #[command(subcommand)]
    pub action: DaemonAction,
}

#[derive(clap::Subcommand)]
pub enum DaemonAction {
    /// Start the kaspa-auth daemon service
    Start(DaemonStartCommand),
    /// Stop the running daemon
    Stop(DaemonStopCommand),
    /// Check daemon status
    Status(DaemonStatusCommand),
    /// Send command to running daemon
    Send(DaemonSendCommand),
}

#[derive(Args)]
pub struct DaemonStartCommand {
    /// Socket path for IPC communication
    #[arg(long, default_value = "/tmp/kaspa-auth.sock")]
    pub socket_path: String,

    /// Directory to store wallet and other data
    #[arg(long, default_value = ".")]
    pub data_dir: String,

    /// Auto-unlock identities on startup
    #[arg(long)]
    pub auto_unlock: bool,

    /// Session timeout in seconds
    #[arg(long, default_value = "3600")]
    pub session_timeout: u64,

    /// Run in foreground (don't daemonize)
    #[arg(long)]
    pub foreground: bool,

    // Storage options (set by CLI flags)
    #[arg(skip)]
    pub use_keychain: bool,

    #[arg(skip)]
    pub dev_mode: bool,
}

#[derive(Args)]
pub struct DaemonStopCommand {
    /// Socket path to connect to
    #[arg(long, default_value = "/tmp/kaspa-auth.sock")]
    pub socket_path: String,
}

#[derive(Args)]
pub struct DaemonStatusCommand {
    /// Socket path to connect to
    #[arg(long, default_value = "/tmp/kaspa-auth.sock")]
    pub socket_path: String,
}

#[derive(Args)]
pub struct DaemonSendCommand {
    /// Socket path to connect to
    #[arg(long, default_value = "/tmp/kaspa-auth.sock")]
    pub socket_path: String,

    /// Command to send
    #[command(subcommand)]
    pub command: DaemonParticipantPeerCommand,
}

#[derive(clap::Subcommand)]
pub enum DaemonParticipantPeerCommand {
    /// Ping the daemon
    Ping,
    /// Unlock an identity
    Unlock {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
    },
    /// Lock all identities
    Lock,
    /// Create new identity
    Create {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
    },
    /// Sign challenge
    Sign {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        challenge: String,
    },
    /// Authenticate with organizer peer
    Auth {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        peer_url: String,
    },
    /// List available identities
    List,
    /// List active sessions
    Sessions,
}

impl DaemonCommand {
    pub fn set_storage_options(&mut self, use_keychain: bool, dev_mode: bool) {
        if let DaemonAction::Start(cmd) = &mut self.action {
            cmd.use_keychain = use_keychain;
            cmd.dev_mode = dev_mode;
        }
        // Other commands don't need storage options
    }

    pub async fn execute(self) -> Result<(), Box<dyn Error>> {
        match self.action {
            DaemonAction::Start(cmd) => cmd.execute().await,
            DaemonAction::Stop(cmd) => cmd.execute().await,
            DaemonAction::Status(cmd) => cmd.execute().await,
            DaemonAction::Send(cmd) => cmd.execute().await,
        }
    }
}

impl DaemonStartCommand {
    pub async fn execute(self) -> Result<(), Box<dyn Error>> {
        println!("🚀 Starting kaspa-auth daemon");

        let config = DaemonConfig {
            data_dir: self.data_dir.clone(),
            socket_path: self.socket_path.clone(),
            auto_unlock: self.auto_unlock,
            session_timeout: self.session_timeout,
            use_keychain: self.use_keychain,
            dev_mode: self.dev_mode,
        };

        if self.foreground {
            println!("🖥️ Running in foreground mode");
            let daemon = AuthDaemon::new(config);
            daemon.run().await?;
        } else {
            println!("🌙 Daemonizing process...");
            // TODO: Implement proper daemonization
            println!("⚠️ Foreground mode only for now. Use --foreground");
            let daemon = AuthDaemon::new(config);
            daemon.run().await?;
        }

        Ok(())
    }
}

impl DaemonStopCommand {
    pub async fn execute(self) -> Result<(), Box<dyn Error>> {
        println!("🛑 Stopping kaspa-auth daemon");

        let request = DaemonRequest::Shutdown;
        match send_daemon_request(&self.socket_path, request).await {
            Ok(_) => {
                println!("✅ Daemon shutdown initiated");
                Ok(())
            }
            Err(e) => {
                println!("❌ Failed to stop daemon: {e}");
                Err(e)
            }
        }
    }
}

impl DaemonStatusCommand {
    pub async fn execute(self) -> Result<(), Box<dyn Error>> {
        println!("📊 Checking daemon status");

        let request = DaemonRequest::Status;
        match send_daemon_request(&self.socket_path, request).await {
            Ok(DaemonResponse::Status { is_unlocked, loaded_identities, active_sessions }) => {
                println!("✅ Daemon is running");
                println!("🔓 Unlocked: {}", if is_unlocked { "Yes" } else { "No" });
                println!("👥 Loaded identities: {}", loaded_identities.len());
                for identity in loaded_identities {
                    println!("   - {identity}");
                }
                println!("🔗 Active sessions: {active_sessions}");
                Ok(())
            }
            Ok(response) => {
                println!("❌ Unexpected response: {response:?}");
                Err("Unexpected response".into())
            }
            Err(e) => {
                println!("❌ Daemon not running or not responding: {e}");
                Err(e)
            }
        }
    }
}

impl DaemonSendCommand {
    pub async fn execute(self) -> Result<(), Box<dyn Error>> {
        let request = match self.command {
            DaemonParticipantPeerCommand::Ping => DaemonRequest::Ping,
            DaemonParticipantPeerCommand::Unlock { username, password } => DaemonRequest::Unlock { password, username },
            DaemonParticipantPeerCommand::Lock => DaemonRequest::Lock,
            DaemonParticipantPeerCommand::Create { username, password } => DaemonRequest::CreateIdentity { username, password },
            DaemonParticipantPeerCommand::Sign { username, challenge } => DaemonRequest::SignChallenge { challenge, username },
            DaemonParticipantPeerCommand::Auth { username, peer_url } => DaemonRequest::Authenticate { peer_url, username },
            DaemonParticipantPeerCommand::List => DaemonRequest::ListIdentities,
            DaemonParticipantPeerCommand::Sessions => DaemonRequest::ListSessions,
        };

        match send_daemon_request(&self.socket_path, request).await {
            Ok(response) => {
                match response {
                    DaemonResponse::Success { message } => {
                        println!("✅ {message}");
                    }
                    DaemonResponse::Error { error } => {
                        println!("❌ Error: {error}");
                    }
                    DaemonResponse::Pong { version, uptime_seconds, identities_loaded } => {
                        println!("🏓 Pong!");
                        println!("📊 Version: {version}");
                        println!("⏱️ Uptime: {uptime_seconds}s");
                        println!("👥 Identities loaded: {identities_loaded}");
                    }
                    DaemonResponse::Signature { signature, public_key } => {
                        println!("✍️ Signature: {signature}");
                        println!("🔑 Public key: {public_key}");
                    }
                    DaemonResponse::AuthResult { success, episode_id, session_token, message } => {
                        println!("🔐 Authentication: {}", if success { "Success" } else { "Failed" });
                        if let Some(id) = episode_id {
                            println!("📧 Episode ID: {id}");
                        }
                        if let Some(token) = session_token {
                            println!("🎫 Session token: {token}");
                        }
                        println!("📝 Message: {message}");
                    }
                    DaemonResponse::Identities { usernames } => {
                        println!("👥 Available identities:");
                        for username in usernames {
                            println!("   - {username}");
                        }
                    }
                    DaemonResponse::Sessions { sessions } => {
                        println!("🔗 Active sessions: {}", sessions.len());
                        if sessions.is_empty() {
                            println!("   (No active sessions)");
                        } else {
                            for session in sessions {
                                println!(
                                    "   - Episode {}: {} @ {} ({}s ago)",
                                    session.episode_id, session.username, session.peer_url, session.created_at_seconds
                                );
                                println!(
                                    "     Token: {}...",
                                    &session.session_token[..std::cmp::min(16, session.session_token.len())]
                                );
                            }
                        }
                    }
                    _ => {
                        println!("📨 Response: {response:?}");
                    }
                }
                Ok(())
            }
            Err(e) => {
                println!("❌ Communication error: {e}");
                Err(e)
            }
        }
    }
}

/// Send request to daemon and receive response
async fn send_daemon_request(socket_path: &str, request: DaemonRequest) -> Result<DaemonResponse, Box<dyn Error>> {
    // Connect to daemon socket
    let mut stream = create_participant_peer_connection(socket_path).await?;

    // Send request
    let request_msg = IpcMessage::new(request);
    let request_bytes = serialize_message(&request_msg)?;
    stream.write_all(&request_bytes).await?;

    // Read response
    let mut buffer = vec![0u8; 8192];
    let bytes_read = stream.read(&mut buffer).await?;

    // Parse response
    let response_msg: IpcMessage<DaemonResponse> = deserialize_message(&buffer[..bytes_read])?;

    Ok(response_msg.payload)
}

/// Create platform-specific participant peer connection
#[cfg(unix)]
async fn create_participant_peer_connection(socket_path: &str) -> Result<PlatformStream, Box<dyn Error>> {
    Ok(UnixStream::connect(socket_path).await?)
}

#[cfg(windows)]
async fn create_participant_peer_connection(_socket_path: &str) -> Result<PlatformStream, Box<dyn Error>> {
    // On Windows, connect to TCP socket on localhost
    let port = 8901; // Must match the port used in service.rs
    let addr = format!("127.0.0.1:{port}");
    Ok(TcpStream::connect(addr).await?)
}
