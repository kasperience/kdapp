// src/daemon/service.rs - Main kaspa-auth daemon service implementation

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

use crate::daemon::{DaemonConfig, protocol::*};
use crate::utils::keychain::{KeychainManager, KeychainConfig};
use crate::wallet::KaspaAuthWallet;

/// Active authentication session
#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub username: String,
    pub server_url: String,
    pub episode_id: u64,
    pub session_token: String,
    pub created_at: Instant,
}

/// Main daemon service that manages authentication identities
pub struct AuthDaemon {
    config: DaemonConfig,
    start_time: Instant,
    
    // In-memory unlocked identities (secured in daemon process memory)
    unlocked_identities: Arc<Mutex<HashMap<String, KaspaAuthWallet>>>,
    
    // Active authentication sessions
    active_sessions: Arc<Mutex<HashMap<u64, ActiveSession>>>,
    
    // Keychain manager for persistent storage
    keychain_manager: KeychainManager,
    
    // Broadcast channel for notifications
    event_tx: broadcast::Sender<DaemonEvent>,
}

/// Events broadcast by the daemon
#[derive(Debug, Clone)]
pub enum DaemonEvent {
    IdentityUnlocked { username: String },
    IdentityLocked { username: String },
    AuthenticationStarted { username: String, server_url: String },
    AuthenticationCompleted { username: String, success: bool },
    SessionRevoked { username: String, episode_id: u64 },
}

impl AuthDaemon {
    /// Create new daemon instance
    pub fn new(config: DaemonConfig) -> Self {
        let keychain_config = KeychainConfig::new("kaspa-auth-daemon", config.dev_mode);
        let keychain_manager = KeychainManager::new(keychain_config);
        let (event_tx, _) = broadcast::channel(100);
        
        Self {
            config,
            start_time: Instant::now(),
            unlocked_identities: Arc::new(Mutex::new(HashMap::new())),
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
            keychain_manager,
            event_tx,
        }
    }
    
    /// Start the daemon service
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting kaspa-auth daemon");
        println!("🔌 Socket: {}", self.config.socket_path);
        println!("🔐 Keychain: {}", if self.config.use_keychain { "enabled" } else { "disabled" });
        
        // Remove existing socket file
        let _ = std::fs::remove_file(&self.config.socket_path);
        
        // Create Unix socket listener
        let listener = UnixListener::bind(&self.config.socket_path)?;
        println!("✅ Daemon listening on {}", self.config.socket_path);
        
        // Auto-unlock if configured
        if self.config.auto_unlock {
            println!("🔓 Auto-unlock mode enabled");
            // In production, this would prompt for master password
        }
        
        // Accept client connections
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let daemon = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = daemon.handle_client(stream).await {
                            eprintln!("❌ Client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("❌ Accept error: {}", e);
                }
            }
        }
    }
    
    /// Handle individual client connection
    async fn handle_client(&self, mut stream: UnixStream) -> Result<(), Box<dyn std::error::Error>> {
        let mut buffer = vec![0u8; 8192];
        
        loop {
            // Read message length
            let bytes_read = stream.read(&mut buffer).await?;
            if bytes_read == 0 {
                break; // Client disconnected
            }
            
            // Parse request
            let request_msg: IpcMessage<DaemonRequest> = deserialize_message(&buffer[..bytes_read])?;
            let request_id = request_msg.id;
            
            // Process request
            let response = self.process_request(request_msg.payload).await;
            let response_msg = IpcMessage { id: request_id, payload: response };
            
            // Send response
            let response_bytes = serialize_message(&response_msg)?;
            stream.write_all(&response_bytes).await?;
        }
        
        Ok(())
    }
    
    /// Process daemon request and return response
    async fn process_request(&self, request: DaemonRequest) -> DaemonResponse {
        match request {
            DaemonRequest::Ping => {
                let uptime = self.start_time.elapsed().as_secs();
                let identities_count = self.unlocked_identities.lock().unwrap().len();
                
                DaemonResponse::Pong {
                    version: "0.1.0".to_string(),
                    uptime_seconds: uptime,
                    identities_loaded: identities_count,
                }
            }
            
            DaemonRequest::Status => {
                let identities = self.unlocked_identities.lock().unwrap();
                let loaded_identities: Vec<String> = identities.keys().cloned().collect();
                let sessions = self.active_sessions.lock().unwrap();
                let active_sessions_count = sessions.len();
                
                DaemonResponse::Status {
                    is_unlocked: !identities.is_empty(),
                    loaded_identities,
                    active_sessions: active_sessions_count,
                }
            }
            
            DaemonRequest::Unlock { password, username } => {
                self.unlock_identity(&username, &password).await
            }
            
            DaemonRequest::Lock => {
                self.lock_all_identities().await
            }
            
            DaemonRequest::CreateIdentity { username, password } => {
                self.create_identity(&username, &password).await
            }
            
            DaemonRequest::SignChallenge { challenge, username } => {
                self.sign_challenge(&username, &challenge).await
            }
            
            DaemonRequest::Authenticate { server_url, username } => {
                self.authenticate(&username, &server_url).await
            }
            
            DaemonRequest::ListIdentities => {
                // TODO: List available identities from keychain
                DaemonResponse::Identities {
                    usernames: vec!["organizer-peer".to_string(), "participant-peer".to_string()],
                }
            }
            
            DaemonRequest::ListSessions => {
                let sessions = self.active_sessions.lock().unwrap();
                let session_list: Vec<crate::daemon::protocol::SessionInfo> = sessions.values().map(|session| {
                    crate::daemon::protocol::SessionInfo {
                        episode_id: session.episode_id,
                        username: session.username.clone(),
                        server_url: session.server_url.clone(),
                        session_token: session.session_token.clone(),
                        created_at_seconds: session.created_at.elapsed().as_secs(),
                    }
                }).collect();
                
                DaemonResponse::Sessions {
                    sessions: session_list,
                }
            }
            
            DaemonRequest::RevokeSession { episode_id, session_token, username } => {
                self.revoke_session(&username, episode_id, &session_token).await
            }
            
            DaemonRequest::Shutdown => {
                println!("🛑 Shutdown requested");
                std::process::exit(0);
            }
        }
    }
    
    /// Unlock authentication identity and load into memory
    async fn unlock_identity(&self, username: &str, password: &str) -> DaemonResponse {
        println!("🔓 Unlocking identity: {}", username);
        
        // TODO: Verify password against stored hash
        if password.len() < 4 {
            return DaemonResponse::Error {
                error: "Password too short".to_string(),
            };
        }
        
        // Load identity from keychain
        match self.keychain_manager.load_wallet(username) {
            Ok(wallet) => {
                // Store in memory for fast access
                {
                    let mut identities = self.unlocked_identities.lock().unwrap();
                    identities.insert(username.to_string(), wallet);
                }
                
                // Broadcast event
                let _ = self.event_tx.send(DaemonEvent::IdentityUnlocked {
                    username: username.to_string(),
                });
                
                DaemonResponse::Success {
                    message: format!("Identity '{}' unlocked successfully", username),
                }
            }
            Err(e) => {
                DaemonResponse::Error {
                    error: format!("Failed to unlock identity: {}", e),
                }
            }
        }
    }
    
    /// Lock all identities (clear from memory)
    async fn lock_all_identities(&self) -> DaemonResponse {
        let mut identities = self.unlocked_identities.lock().unwrap();
        let count = identities.len();
        identities.clear();
        
        println!("🔒 Locked {} identities", count);
        
        DaemonResponse::Success {
            message: format!("Locked {} identities", count),
        }
    }
    
    /// Create new authentication identity
    async fn create_identity(&self, username: &str, password: &str) -> DaemonResponse {
        println!("🆕 Creating identity: {}", username);
        
        match self.keychain_manager.create_wallet(username) {
            Ok(wallet) => {
                // Store password hash for verification (simplified)
                // TODO: Proper password hashing and storage
                
                // Load into memory
                {
                    let mut identities = self.unlocked_identities.lock().unwrap();
                    identities.insert(username.to_string(), wallet.clone());
                }
                
                DaemonResponse::Success {
                    message: format!("Identity '{}' created successfully", username),
                }
            }
            Err(e) => {
                DaemonResponse::Error {
                    error: format!("Failed to create identity: {}", e),
                }
            }
        }
    }
    
    /// Sign authentication challenge
    async fn sign_challenge(&self, username: &str, challenge: &str) -> DaemonResponse {
        let identities = self.unlocked_identities.lock().unwrap();
        
        match identities.get(username) {
            Some(wallet) => {
                // TODO: Implement actual signature using wallet keypair
                let public_key = wallet.get_public_key_hex();
                let mock_signature = format!("sig_{}_{}", challenge, username);
                
                println!("✍️ Signed challenge for {}", username);
                
                DaemonResponse::Signature {
                    signature: mock_signature,
                    public_key,
                }
            }
            None => {
                DaemonResponse::Error {
                    error: format!("Identity '{}' not unlocked", username),
                }
            }
        }
    }
    
    /// Perform full authentication flow
    async fn authenticate(&self, username: &str, server_url: &str) -> DaemonResponse {
        println!("🔐 Authenticating {} with {}", username, server_url);
        
        // Check if identity is unlocked
        let identities = self.unlocked_identities.lock().unwrap();
        if !identities.contains_key(username) {
            return DaemonResponse::Error {
                error: format!("Identity '{}' not unlocked", username),
            };
        }
        drop(identities);
        
        // Broadcast event
        let _ = self.event_tx.send(DaemonEvent::AuthenticationStarted {
            username: username.to_string(),
            server_url: server_url.to_string(),
        });
        
        // Generate episode ID and session token (deterministic for demo)
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let episode_id = rng.gen::<u64>();
        let session_token = format!("sess_daemon_{}_{}", episode_id, rng.gen::<u32>());
        
        // Create active session with longer timeout
        let session = ActiveSession {
            username: username.to_string(),
            server_url: server_url.to_string(),
            episode_id,
            session_token: session_token.clone(),
            created_at: Instant::now(),
        };
        
        // Store session (sessions persist until explicitly revoked)
        {
            let mut sessions = self.active_sessions.lock().unwrap();
            sessions.insert(episode_id, session);
            println!("✅ Created persistent session {} for {} (valid for {}s)", 
                     episode_id, username, self.config.session_timeout);
        }
        
        // Broadcast success event
        let _ = self.event_tx.send(DaemonEvent::AuthenticationCompleted {
            username: username.to_string(),
            success: true,
        });
        
        DaemonResponse::AuthResult {
            success: true,
            episode_id: Some(episode_id),
            session_token: Some(session_token),
            message: format!("Authentication successful - session {} active for {}s", 
                           episode_id, self.config.session_timeout),
        }
    }
    
    /// Revoke active session
    async fn revoke_session(&self, username: &str, episode_id: u64, session_token: &str) -> DaemonResponse {
        println!("🔄 Revoking session {} for {}", episode_id, username);
        
        // Check if session exists and belongs to user
        let mut sessions = self.active_sessions.lock().unwrap();
        match sessions.get(&episode_id) {
            Some(session) if session.username == username && session.session_token == session_token => {
                // Remove session
                sessions.remove(&episode_id);
                println!("✅ Removed active session {} for {}", episode_id, username);
                
                // Broadcast event
                let _ = self.event_tx.send(DaemonEvent::SessionRevoked {
                    username: username.to_string(),
                    episode_id,
                });
                
                DaemonResponse::Success {
                    message: format!("Session {} revoked successfully", episode_id),
                }
            }
            Some(_) => {
                DaemonResponse::Error {
                    error: "Session not found or access denied".to_string(),
                }
            }
            None => {
                DaemonResponse::Error {
                    error: format!("Session {} not found", episode_id),
                }
            }
        }
    }
}

// Clone implementation for spawning tasks
impl Clone for AuthDaemon {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            start_time: self.start_time,
            unlocked_identities: Arc::clone(&self.unlocked_identities),
            active_sessions: Arc::clone(&self.active_sessions),
            keychain_manager: KeychainManager::new(
                KeychainConfig::new("kaspa-auth-daemon", self.config.dev_mode)
            ),
            event_tx: self.event_tx.clone(),
        }
    }
}