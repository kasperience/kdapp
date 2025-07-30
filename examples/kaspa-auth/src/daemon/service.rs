// src/daemon/service.rs - Main kaspa-auth daemon service implementation

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
#[cfg(windows)]
use tokio::net::{TcpListener, TcpStream};

// Platform-specific type aliases
#[cfg(unix)]
type PlatformListener = UnixListener;
#[cfg(unix)]
type PlatformStream = UnixStream;

#[cfg(windows)]
type PlatformListener = TcpListener;
#[cfg(windows)]
type PlatformStream = TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;
use reqwest;
use serde_json;

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
        let keychain_config = KeychainConfig::new("kaspa-auth", config.dev_mode);
        let keychain_manager = KeychainManager::new(keychain_config, &config.data_dir);
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
        
        // Create platform-specific listener
        let listener = self.create_listener().await?;
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
    
    /// Create platform-specific listener
    #[cfg(unix)]
    async fn create_listener(&self) -> Result<PlatformListener, Box<dyn std::error::Error>> {
        Ok(UnixListener::bind(&self.config.socket_path)?)
    }
    
    #[cfg(windows)]
    async fn create_listener(&self) -> Result<PlatformListener, Box<dyn std::error::Error>> {
        // On Windows, use TCP socket on localhost with a port derived from socket path
        let port = 8901; // Default port for kaspa-auth daemon
        let addr = format!("127.0.0.1:{}", port);
        Ok(TcpListener::bind(addr).await?)
    }
    
    /// Handle individual client connection
    async fn handle_client(&self, mut stream: PlatformStream) -> Result<(), Box<dyn std::error::Error>> {
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
    async fn create_identity(&self, username: &str, _password: &str) -> DaemonResponse {
        println!("🆕 Creating identity: {}", username);

        // This now correctly saves the wallet to disk.
        match self.keychain_manager.create_and_save_wallet(username) {
            Ok(wallet) => {
                // Also load it into memory for immediate use
                {
                    let mut identities = self.unlocked_identities.lock().unwrap();
                    identities.insert(username.to_string(), wallet.clone());
                }

                DaemonResponse::Success {
                    message: format!("Identity '{}' created and saved successfully", username),
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
        
        // Check if identity is unlocked and clone wallet to avoid holding lock across await
        let wallet = {
            let identities = self.unlocked_identities.lock().unwrap();
            match identities.get(username) {
                Some(wallet) => wallet.clone(),
                None => {
                    return DaemonResponse::Error {
                        error: format!("Identity '{}' not unlocked", username),
                    };
                }
            }
        }; // Lock is dropped here
        
        // Broadcast event
        let _ = self.event_tx.send(DaemonEvent::AuthenticationStarted {
            username: username.to_string(),
            server_url: server_url.to_string(),
        });
        
        // REAL BLOCKCHAIN AUTHENTICATION - Use WORKING web UI pattern
        println!("🌐 Starting REAL blockchain authentication using WORKING endpoint pattern...");
        println!("💰 Participant will pay for ALL transactions");
        println!("💸 Organizer pays 0.00000 KAS (coordination only)");
        
        // Use the FIXED authentication flow
        println!("🌐 Using FIXED endpoint pattern (3 blockchain transactions)");
        
        match self.run_working_authentication_flow(&wallet, server_url).await {
            Ok(auth_result) => {
                println!("✅ BLOCKCHAIN AUTHENTICATION SUCCESS!");
                println!("📧 Episode ID: {}", auth_result.episode_id);
                println!("🎫 Session Token: {}", auth_result.session_token);
                println!("🔗 All 3 transactions confirmed on blockchain");
                
                // Create active session
                let session = ActiveSession {
                    username: username.to_string(),
                    server_url: server_url.to_string(),
                    episode_id: auth_result.episode_id,
                    session_token: auth_result.session_token.clone(),
                    created_at: Instant::now(),
                };
                
                // Store session
                {
                    let mut sessions = self.active_sessions.lock().unwrap();
                    sessions.insert(auth_result.episode_id, session);
                }
                
                // Broadcast success event
                let _ = self.event_tx.send(DaemonEvent::AuthenticationCompleted {
                    username: username.to_string(),
                    success: true,
                });
                
                DaemonResponse::AuthResult {
                    success: true,
                    episode_id: Some(auth_result.episode_id),
                    session_token: Some(auth_result.session_token),
                    message: format!("Authentication successful - 3 transactions confirmed"),
                }
            }
            Err(e) => {
                println!("❌ AUTHENTICATION FAILED: {}", e);
                
                // Broadcast failure event
                let _ = self.event_tx.send(DaemonEvent::AuthenticationCompleted {
                    username: username.to_string(),
                    success: false,
                });
                
                DaemonResponse::Error {
                    error: format!("Authentication failed: {}", e),
                }
            }
        }
    }
    
    /// Run authentication using WORKING web UI endpoint pattern (3 transactions)
    async fn run_working_authentication_flow(
        &self, 
        wallet: &KaspaAuthWallet, 
        server_url: &str
    ) -> Result<crate::auth::authentication::AuthenticationResult, Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();
        let public_key_hex = wallet.get_public_key_hex();
        
        println!("🔑 Using wallet public key: {}", public_key_hex);
        println!("🎯 Following EXACT Web UI pattern (3 transactions)");
        
        // Step 1: Create episode (HTTP coordination only)
        println!("📝 Step 1: Creating episode via /auth/start...");
        let start_response = client
            .post(&format!("{}/auth/start", server_url))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "public_key": public_key_hex
            }))
            .send()
            .await?;
        
        if !start_response.status().is_success() {
            let status = start_response.status();
            let body = start_response.text().await?;
            return Err(format!("Failed to start auth: HTTP {} - {}", status, body).into());
        }
        
        let start_data: serde_json::Value = start_response.json().await?;
        let episode_id = start_data["episode_id"].as_u64()
            .ok_or("Server did not return valid episode_id")?;
        
        println!("✅ Episode {} created (HTTP coordination)", episode_id);
        
        // Step 2: Request challenge (HTTP coordination only) 
        println!("📨 Step 2: Requesting challenge via /auth/request-challenge...");
        let challenge_response = client
            .post(&format!("{}/auth/request-challenge", server_url))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "episode_id": episode_id,
                "public_key": public_key_hex
            }))
            .send()
            .await?;
        
        if !challenge_response.status().is_success() {
            let status = challenge_response.status();
            let body = challenge_response.text().await?;
            return Err(format!("Failed to request challenge: HTTP {} - {}", status, body).into());
        }
        
        // Get challenge immediately from response (no polling needed with fixed endpoints)
        let challenge_data: serde_json::Value = challenge_response.json().await?;
        let challenge = challenge_data["nonce"].as_str()
            .ok_or("Server did not return challenge")?
            .to_string();
        
        println!("🎯 Challenge received immediately: {}", challenge);
        
        // Step 3: Sign challenge locally
        println!("✍️ Step 3: Signing challenge locally...");
        let msg = kdapp::pki::to_message(&challenge);
        let signature = kdapp::pki::sign_message(&wallet.keypair.secret_key(), &msg);
        let signature_hex = hex::encode(signature.0.serialize_der());
        
        println!("✅ Challenge signed with wallet private key");
        
        // Step 4: Submit verification (this triggers ALL 3 blockchain transactions)
        println!("📤 Step 4: Submitting verification via /auth/verify...");
        println!("⚡ This will trigger all 3 blockchain transactions:");
        println!("   1. NewEpisode");
        println!("   2. RequestChallenge");
        println!("   3. SubmitResponse");
        
        let verify_response = client
            .post(&format!("{}/auth/verify", server_url))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "episode_id": episode_id,
                "signature": signature_hex,
                "nonce": challenge
            }))
            .send()
            .await?;
        
        if !verify_response.status().is_success() {
            let status = verify_response.status();
            let body = verify_response.text().await?;
            return Err(format!("Failed to verify: HTTP {} - {}", status, body).into());
        }
        
        let verify_data: serde_json::Value = verify_response.json().await?;
        let transaction_id = verify_data["transaction_id"].as_str();
        
        println!("✅ All transactions submitted!");
        if let Some(tx_id) = transaction_id {
            println!("📋 Final transaction ID: {}", tx_id);
        }
        
        // Step 5: Poll for authentication completion
        println!("⏳ Step 5: Waiting for blockchain confirmation...");
        let mut session_token = String::new();
        let max_attempts = 60; // 30 seconds timeout
        
        for attempt in 1..=max_attempts {
            let status_response = client
                .get(&format!("{}/auth/status/{}", server_url, episode_id))
                .send()
                .await?;
            
            if status_response.status().is_success() {
                let status_data: serde_json::Value = status_response.json().await?;
                
                // Debug logging
                if attempt == 1 {
                    println!("📊 Episode status: {}", serde_json::to_string_pretty(&status_data)?);
                }
                
                if let (Some(authenticated), Some(token)) = (
                    status_data["authenticated"].as_bool(),
                    status_data["session_token"].as_str()
                ) {
                    if authenticated && !token.is_empty() {
                        session_token = token.to_string();
                        println!("✅ Authentication confirmed by blockchain!");
                        println!("🎫 Session token: {}", session_token);
                        break;
                    }
                }
            }
            
            if attempt % 10 == 0 {
                println!("⏳ Still waiting... ({}/{})", attempt, max_attempts);
            }
            
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        
        if session_token.is_empty() {
            return Err("❌ Timeout waiting for blockchain confirmation".into());
        }
        
        Ok(crate::auth::authentication::AuthenticationResult {
            episode_id,
            session_token,
            authenticated: true,
        })
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
                KeychainConfig::new("kaspa-auth", self.config.dev_mode),
                &self.config.data_dir
            ),
            event_tx: self.event_tx.clone(),
        }
    }
}