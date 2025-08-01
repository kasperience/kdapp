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
use rand::Rng;
use rand;


use crate::daemon::{DaemonConfig, protocol::*};
use crate::utils::keychain::{KeychainManager, KeychainConfig};
use crate::wallet::KaspaAuthWallet;

/// Active authentication session
#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub username: String,
    pub peer_url: String,
    pub episode_id: kaspa_hashes::Hash,
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
    AuthenticationStarted { username: String, peer_url: String },
    AuthenticationCompleted { username: String, success: bool },
    SessionRevoked { username: String, episode_id: kaspa_hashes::Hash },
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
        
        // Accept participant connections
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let daemon = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = daemon.handle_participant_peer(stream).await {
                            eprintln!("❌ Participant peer error: {}", e);
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
    
    /// Handle individual participant connection
    async fn handle_participant_peer(&self, mut stream: PlatformStream) -> Result<(), Box<dyn std::error::Error>> {
        let mut buffer = vec![0u8; 8192];
        
        loop {
            // Read message length
            let bytes_read = stream.read(&mut buffer).await?;
            if bytes_read == 0 {
                break; // Participant disconnected
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
            
            DaemonRequest::Authenticate { peer_url, username } => {
                self.authenticate(&username, &peer_url).await
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
                        peer_url: session.peer_url.clone(),
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
        match self.keychain_manager.create_wallet(username) {
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
    async fn authenticate(&self, username: &str, peer_url: &str) -> DaemonResponse {
        println!("🔐 Authenticating {} with {}", username, peer_url);

        let wallet = match self.unlocked_identities.lock().unwrap().get(username) {
            Some(wallet) => wallet.clone(),
            None => return DaemonResponse::Error {
                error: format!("Identity '{}' not unlocked", username),
            },
        };

        let _ = self.event_tx.send(DaemonEvent::AuthenticationStarted {
            username: username.to_string(),
            peer_url: peer_url.to_string(),
        });

        println!("🌐 Starting P2P blockchain authentication flow...");
        println!("💰 Participant (daemon) is responsible for all transaction submissions.");

        let auth_result = match self.run_p2p_authentication_flow(&wallet, peer_url).await {
            Ok(result) => result,
            Err(e) => {
                println!("❌ AUTHENTICATION FAILED: {}", e);
                let _ = self.event_tx.send(DaemonEvent::AuthenticationCompleted {
                    username: username.to_string(),
                    success: false,
                });
                return DaemonResponse::Error {
                    error: format!("Authentication failed: {}", e),
                };
            }
        };

        println!("✅ P2P AUTHENTICATION SUCCESS!");
        println!("📧 Episode ID: {}", auth_result.episode_id);
        println!("🎫 Session Token: {}", auth_result.session_token);

        let mut episode_id_bytes = [0u8; 32];
        episode_id_bytes[..8].copy_from_slice(&auth_result.episode_id.to_le_bytes());
        let episode_hash = kaspa_hashes::Hash::from_bytes(episode_id_bytes);

        let session = ActiveSession {
            username: username.to_string(),
            peer_url: peer_url.to_string(),
            episode_id: episode_hash,
            session_token: auth_result.session_token.clone(),
            created_at: Instant::now(),
        };

        self.active_sessions.lock().unwrap().insert(auth_result.episode_id, session);

        let _ = self.event_tx.send(DaemonEvent::AuthenticationCompleted {
            username: username.to_string(),
            success: true,
        });

        DaemonResponse::AuthResult {
            success: true,
            episode_id: Some(auth_result.episode_id),
            session_token: Some(auth_result.session_token),
            message: "Authentication successful - P2P flow completed".to_string(),
        }
    }

    /// Run authentication using P2P flow where the daemon submits transactions
    async fn run_p2p_authentication_flow(
        &self,
        wallet: &KaspaAuthWallet,
        peer_url: &str,
    ) -> Result<crate::auth::authentication::AuthenticationResult, Box<dyn std::error::Error>> {
        
use kdapp::engine::EpisodeMessage;
use kdapp::pki::PubKey;
use kdapp::generator::TransactionGenerator;
use kdapp::proxy::connect_client;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::{network::NetworkId, tx::{TransactionOutpoint, UtxoEntry}};
use kaspa_wrpc_client::prelude::*;
use kaspa_rpc_core::api::rpc::RpcApi;
use crate::core::{commands::AuthCommand, episode::SimpleAuth};
use crate::episode_runner::{AUTH_PATTERN, AUTH_PREFIX};


        let participant_pubkey = kdapp::pki::PubKey(wallet.keypair.public_key());
        println!("🔑 Auth public key: {}", participant_pubkey);

        // Connect to Kaspa network
        let network = NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 10);
        println!("📡 Connecting to testnet-10 blockchain...");
        let kaspad = connect_client(network, None).await?;

        // Create Kaspa address for funding transactions
        let kaspa_addr = Address::new(Prefix::Testnet, Version::PubKey, &wallet.keypair.x_only_public_key().0.serialize());
        println!("💰 Kaspa address: {}", kaspa_addr);

        // Get UTXOs for transaction funding
        println!("🔍 Fetching UTXOs...");
        let entries = kaspad.get_utxos_by_addresses(vec![kaspa_addr.clone()]).await?;
        if entries.is_empty() {
            return Err(format!("No UTXOs found for address: {}. Please fund this address with testnet KAS.", kaspa_addr).into());
        }
        let mut utxo = entries.first().map(|entry| {
            (TransactionOutpoint::from(entry.outpoint.clone()), UtxoEntry::from(entry.utxo_entry.clone()))
        }).unwrap();
        println!("✅ UTXO found: {}", utxo.0);

        





        // Create real transaction generator
        let generator = TransactionGenerator::new(wallet.keypair, AUTH_PATTERN, AUTH_PREFIX);

        // Step 1: Participant (daemon) creates and submits the NewEpisode transaction to the blockchain
        let episode_id = rand::random::<u32>(); // Generate a random u32 for episode_id
        let new_episode_msg = kdapp::engine::EpisodeMessage::<SimpleAuth>::NewEpisode {
            episode_id,
            participants: vec![participant_pubkey],
        };
        let tx = generator.build_command_transaction(utxo, &kaspa_addr, &new_episode_msg, 5000);
        println!("🚀 Submitting NewEpisode transaction: {}", tx.id());
        let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
        utxo = kdapp::generator::get_first_output_utxo(&tx);
        println!("✅ NewEpisode transaction submitted to blockchain. Episode ID: {}", episode_id);

        // Step 2: Send RequestChallenge command to blockchain
        let auth_command = AuthCommand::RequestChallenge;
        let step = kdapp::engine::EpisodeMessage::<SimpleAuth>::new_signed_command(
            episode_id, 
            auth_command, 
            wallet.keypair.secret_key(), 
            participant_pubkey
        );
        let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, 5000);
        println!("🚀 Submitting RequestChallenge transaction: {}", tx.id());
        let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
        utxo = kdapp::generator::get_first_output_utxo(&tx);
        println!("✅ RequestChallenge transaction submitted to blockchain!");

        // Step 3: Poll for challenge (via organizer peer's kdapp engine state)
        println!("⏳ Waiting for challenge...");
        let mut challenge = String::new();
        use reqwest;
        use serde_json;
        for _ in 0..10 {
            let status_url = format!("{}/auth/status/{}", peer_url, episode_id);
            if let Ok(res) = reqwest::Client::new().get(&status_url).send().await {
                if let Ok(status) = res.json::<serde_json::Value>().await {
                    if let Some(c) = status["challenge"].as_str() {
                        challenge = c.to_string();
                        break;
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        if challenge.is_empty() {
            return Err("Could not retrieve challenge from organizer peer".into());
        }
        println!("🎯 Challenge received: {}", challenge);

        // Step 4: Sign challenge and send SubmitResponse command to blockchain
        let msg = kdapp::pki::to_message(&challenge);
        let signature = kdapp::pki::sign_message(&wallet.keypair.secret_key(), &msg);
        let signature_hex = hex::encode(signature.0.serialize_der());
        let auth_command = AuthCommand::SubmitResponse {
            signature: signature_hex,
            nonce: challenge,
        };
        let step = kdapp::engine::EpisodeMessage::<SimpleAuth>::new_signed_command(
            episode_id, 
            auth_command, 
            wallet.keypair.secret_key(), 
            participant_pubkey
        );
        let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, 5000);
        println!("🚀 Submitting SubmitResponse transaction: {}", tx.id());
        let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
        println!("✅ SubmitResponse transaction submitted to blockchain!");

        // Step 5: Poll for session token (via organizer peer's kdapp engine state)
        println!("⏳ Waiting for session token...");
        let mut session_token = String::new();
        for _ in 0..10 {
            let status_url = format!("{}/auth/status/{}", peer_url, episode_id);
            if let Ok(res) = reqwest::Client::new().get(&status_url).send().await {
                if let Ok(status) = res.json::<serde_json::Value>().await {
                    if let Some(token) = status["session_token"].as_str() {
                        if !token.is_empty() {
                            session_token = token.to_string();
                            break;
                        }
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        if session_token.is_empty() {
            return Err("Could not retrieve session token from organizer peer".into());
        }
        println!("🎫 Session token received: {}", session_token);

        Ok(crate::auth::authentication::AuthenticationResult {
            episode_id: episode_id as u64,
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
                    episode_id: kaspa_hashes::Hash::from_le_u64([episode_id, 0, 0, 0]),
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
