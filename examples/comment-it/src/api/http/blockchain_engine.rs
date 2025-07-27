// src/api/http/blockchain_engine.rs
use std::sync::{Arc, atomic::AtomicBool, mpsc};
use std::collections::{HashMap, HashSet};
use tokio::sync::broadcast;
use secp256k1::Keypair;
use kdapp::{
    engine::Engine,
    episode::{EpisodeEventHandler, EpisodeId},
    proxy::connect_client,
    generator::TransactionGenerator,
};
use kaspa_consensus_core::network::{NetworkId, NetworkType};

use crate::core::{AuthWithCommentsEpisode, UnifiedCommand};
use crate::api::http::state::{PeerState, WebSocketMessage, SharedEpisodeState};
use crate::episode_runner::{AUTH_PREFIX, AUTH_PATTERN};
use kaspa_wrpc_client::prelude::RpcApi;

/// The main HTTP coordination peer that runs a real kdapp engine
#[derive(Clone)]
pub struct AuthHttpPeer {
    pub peer_state: PeerState,
    pub network: NetworkId,
    pub exit_signal: Arc<AtomicBool>,
}

impl AuthHttpPeer {
    pub async fn new(
        peer_keypair: Keypair,
        websocket_tx: broadcast::Sender<WebSocketMessage>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let network = NetworkId::with_suffix(NetworkType::Testnet, 10);
        
        let transaction_generator = Arc::new(TransactionGenerator::new(
            peer_keypair,
            AUTH_PATTERN,
            AUTH_PREFIX,
        ));
        
        // Create shared episode state that both engine and HTTP coordination peer can access
        let blockchain_episodes = Arc::new(std::sync::Mutex::new(HashMap::new()));
        
        // Create kaspad client for transaction submission
        let kaspad_client = match connect_client(network, None).await {
            Ok(client) => {
                println!("✅ Connected to Kaspa node for transaction submission");
                Some(Arc::new(client))
            }
            Err(e) => {
                println!("⚠️ Failed to connect to Kaspa node: {}", e);
                println!("📋 Transactions will be created but not submitted");
                None
            }
        };
        
        let mut peer_state = PeerState {
            episodes: Arc::new(std::sync::Mutex::new(HashMap::new())),  // Legacy
            blockchain_episodes: blockchain_episodes.clone(),  // NEW - real blockchain state
            websocket_tx,
            peer_keypair,
            transaction_generator,
            kaspad_client,  // NEW - for actual transaction submission
            auth_http_peer: None, // Will be set after AuthHttpPeer is created
            pending_requests: Arc::new(std::sync::Mutex::new(HashSet::new())),  // NEW - request deduplication
            used_utxos: Arc::new(std::sync::Mutex::new(HashSet::new())),  // NEW - UTXO tracking
        };
        
        let exit_signal = Arc::new(AtomicBool::new(false));
        
        let auth_http_peer = AuthHttpPeer {
            peer_state: peer_state.clone(),
            network,
            exit_signal,
        };
        
        // Set the self reference after the struct is created
        peer_state.auth_http_peer = Some(Arc::new(auth_http_peer.clone()));
        
        Ok(auth_http_peer)
    }
    
    /// Start the blockchain listener - this makes HTTP coordination peer a real kdapp node!
    pub async fn start_blockchain_listener(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel();
        
        // Create the episode handler that will process blockchain updates
        let auth_handler = HttpAuthHandler {
            websocket_tx: self.peer_state.websocket_tx.clone(),
            blockchain_episodes: self.peer_state.blockchain_episodes.clone(),
        };
        
        // Start the kdapp engine in a background task
        let engine_task = {
            let rx = rx;
            tokio::task::spawn_blocking(move || {
                let mut engine = Engine::<AuthWithCommentsEpisode, HttpAuthHandler>::new(rx);
                engine.start(vec![auth_handler]);
            })
        };
        
        // Create engines map for proxy listener
        let engines = std::iter::once((AUTH_PREFIX, (AUTH_PATTERN, tx))).collect();
        
        // Start the blockchain listener using kdapp's proper pattern
        let kaspad = connect_client(self.network, None).await?;
        let exit_signal_clone = self.exit_signal.clone();
        let listener_task = tokio::spawn(async move {
            kdapp::proxy::run_listener(kaspad, engines, exit_signal_clone).await;
        });
        
        println!("🔗 kdapp engine started - HTTP coordination peer is now a real blockchain node!");
        
        // Wait for either task to complete
        tokio::select! {
            _ = engine_task => {
                println!("⚠️ kdapp engine task completed");
            }
            _ = listener_task => {
                println!("⚠️ Blockchain listener task completed");
            }
        }
        
        Ok(())
    }
    
    /// Set the auth peer reference in the peer state
    pub fn set_self_reference(self, _auth_peer: Arc<AuthHttpPeer>) -> Self {
        // This creates a circular reference which is fine for this use case
        // The auth_peer field allows handlers to access the kdapp engine
        // We'll use weak references if needed later
        self
    }
    
    /// Get episode state from the kdapp engine (not memory!)
    pub fn get_episode_state(&self, episode_id: EpisodeId) -> Option<AuthWithCommentsEpisode> {
        println!("🔍 Querying blockchain episode state for episode {}", episode_id);
        
        match self.peer_state.blockchain_episodes.lock() {
            Ok(episodes) => {
                if let Some(episode) = episodes.get(&(episode_id as u64)) {
                    println!("✅ Found episode {} in blockchain state", episode_id);
                    Some(episode.clone())
                } else {
                    println!("⚠️ Episode {} not found in blockchain state", episode_id);
                    None
                }
            }
            Err(e) => {
                println!("❌ Failed to lock blockchain episodes: {}", e);
                None
            }
        }
    }
    
    /// Submit an EpisodeMessage transaction to the blockchain
    pub async fn submit_episode_message_transaction(
        &self,
        episode_message: kdapp::engine::EpisodeMessage<crate::core::AuthWithCommentsEpisode>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(kaspad) = self.peer_state.kaspad_client.as_ref() {
            // CRITICAL FIX: Extract participant's public key from episode message
            let participant_pubkey = match &episode_message {
                kdapp::engine::EpisodeMessage::SignedCommand { pubkey, .. } => {
                    *pubkey
                }
                kdapp::engine::EpisodeMessage::NewEpisode { participants, .. } => {
                    // For NewEpisode, use the first participant as the creator
                    if participants.is_empty() {
                        return Err("NewEpisode has no participants".into());
                    }
                    participants[0]
                }
                _ => {
                    return Err("Episode message variant not supported for transaction submission".into());
                }
            };
            
            // Create participant's Kaspa address from their actual public key
            let participant_addr = kaspa_addresses::Address::new(
                kaspa_addresses::Prefix::Testnet, 
                kaspa_addresses::Version::PubKey, 
                &participant_pubkey.0.serialize()[1..] // Remove compression byte for address
            );
            
            println!("🎯 Using REAL participant address: {}", participant_addr);
            println!("🔑 Participant pubkey: {}", hex::encode(participant_pubkey.0.serialize()));
            
            // Get UTXOs for participant
            let entries = kaspad.get_utxos_by_addresses(vec![participant_addr.clone()]).await?;
            if entries.is_empty() {
                return Err("No UTXOs found for participant wallet. Please fund the wallet.".into());
            }
            
            // 🔧 UTXO FIX: Find first unused UTXO instead of always using first
            let mut selected_utxo = None;
            {
                let used_utxos = self.peer_state.used_utxos.lock().unwrap();
                for entry in &entries {
                    let utxo_id = format!("{}:{}", entry.outpoint.transaction_id, entry.outpoint.index);
                    if !used_utxos.contains(&utxo_id) {
                        selected_utxo = Some((
                            kaspa_consensus_core::tx::TransactionOutpoint::from(entry.outpoint.clone()), 
                            kaspa_consensus_core::tx::UtxoEntry::from(entry.utxo_entry.clone()),
                            utxo_id
                        ));
                        break;
                    }
                }
            }
            
            let (outpoint, utxo_entry, utxo_id) = selected_utxo.ok_or("No unused UTXOs available. Please wait for previous transactions to confirm.")?;
            let utxo = (outpoint, utxo_entry);
            
            // Mark this UTXO as used temporarily
            {
                let mut used_utxos = self.peer_state.used_utxos.lock().unwrap();
                used_utxos.insert(utxo_id.clone());
                println!("🔒 Reserved UTXO: {}", utxo_id);
            }
            
            // Build and submit transaction using the transaction generator
            let tx = self.peer_state.transaction_generator.build_command_transaction(
                utxo, 
                &participant_addr, 
                &episode_message, 
                5000 // fee
            );
            
            // Submit to blockchain
            let submit_result = kaspad.submit_transaction(tx.as_ref().into(), false).await;
            
            match submit_result {
                Ok(_) => {
                    let tx_id = tx.id().to_string();
                    println!("✅ Transaction {} submitted to blockchain successfully!", tx_id);
                    
                    // 🔧 UTXO FIX: Schedule UTXO cleanup after successful submission
                    let used_utxos_cleanup = self.peer_state.used_utxos.clone();
                    let utxo_id_cleanup = utxo_id.clone();
                    tokio::spawn(async move {
                        // Wait 10 seconds then remove from used set (transaction should be confirmed by then)
                        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                        if let Ok(mut used_utxos) = used_utxos_cleanup.lock() {
                            used_utxos.remove(&utxo_id_cleanup);
                            println!("🔓 Released UTXO: {}", utxo_id_cleanup);
                        }
                    });
                    
                    Ok(tx_id)
                }
                Err(e) => {
                    // 🔧 UTXO FIX: Release UTXO immediately if transaction fails
                    {
                        let mut used_utxos = self.peer_state.used_utxos.lock().unwrap();
                        used_utxos.remove(&utxo_id);
                        println!("🔓 Released UTXO due to error: {}", utxo_id);
                    }
                    Err(e.into())
                }
            }
        } else {
            Err("Kaspad client not available for transaction submission.".into())
        }
    }
}

/// Episode event handler that broadcasts updates to WebSocket clients
pub struct HttpAuthHandler {
    pub websocket_tx: broadcast::Sender<WebSocketMessage>,
    pub blockchain_episodes: SharedEpisodeState,
}

impl EpisodeEventHandler<AuthWithCommentsEpisode> for HttpAuthHandler {
    fn on_initialize(&self, episode_id: EpisodeId, episode: &AuthWithCommentsEpisode) {
        println!("🎭 MATRIX UI SUCCESS: Auth episode {} initialized on blockchain", episode_id);
        println!("🎬 Episode {} initialized on blockchain", episode_id);
        
        // Store episode in shared blockchain state
        if let Ok(mut episodes) = self.blockchain_episodes.lock() {
            episodes.insert(episode_id.into(), episode.clone());
            println!("✅ Stored episode {} in blockchain state", episode_id);
        } else {
            println!("❌ Failed to store episode {} in blockchain state", episode_id);
        }
        
        let message = WebSocketMessage {
            message_type: "episode_created".to_string(),
            episode_id: Some(episode_id.into()),
            authenticated: Some(false),
            challenge: episode.challenge(),
            session_token: episode.session_token(),
            comment: None,
            comments: None,
        };
        
        let _ = self.websocket_tx.send(message);
    }
    
    fn on_command(
        &self,
        episode_id: EpisodeId,
        episode: &AuthWithCommentsEpisode,
        cmd: &UnifiedCommand,
        authorization: Option<kdapp::pki::PubKey>,
        _metadata: &kdapp::episode::PayloadMetadata,
    ) {
        println!("⚡ Episode {} updated on blockchain", episode_id);
        println!("🔍 DEBUG: on_command called for episode {} with command: {:?}", episode_id, cmd);
        
        // Read previous state BEFORE updating (for session revocation detection and comment detection)
        let previous_episode = if let Ok(episodes) = self.blockchain_episodes.lock() {
            episodes.get(&(episode_id as u64)).cloned()
        } else {
            None
        };
        
        // Update episode in shared blockchain state
        if let Ok(mut episodes) = self.blockchain_episodes.lock() {
            episodes.insert(episode_id.into(), episode.clone());
            println!("✅ Updated episode {} in blockchain state", episode_id);
        } else {
            println!("❌ Failed to update episode {} in blockchain state", episode_id);
        }
        
        // 🚀 CRITICAL: Check for new comments and broadcast them real-time!
        if let UnifiedCommand::SubmitComment { text, session_token: _ } = cmd {
            println!("💬 NEW COMMENT detected on blockchain for episode {}", episode_id);
            println!("📝 Comment text: \"{}\"", text);
            
            // Find the latest comment (should be the last one added)
            if let Some(latest_comment) = episode.comments.last() {
                println!("🎯 Broadcasting new comment to all connected peers...");
                
                let message = WebSocketMessage {
                    message_type: "new_comment".to_string(),
                    episode_id: Some(episode_id.into()),
                    authenticated: Some(episode.is_authenticated()),
                    challenge: episode.challenge(),
                    session_token: episode.session_token(),
                    comment: Some(crate::api::http::types::CommentData {
                        id: latest_comment.id,
                        text: latest_comment.text.clone(),
                        author: latest_comment.author.clone(),
                        timestamp: latest_comment.timestamp
                    }),
                    comments: None,
                };
                
                let receiver_count = self.websocket_tx.receiver_count();
                let _ = self.websocket_tx.send(message);
                println!("📡 NEW COMMENT broadcasted to {} connected peer(s)! 🎉", receiver_count);
            }
            return; // Don't process as auth command
        }
        
        // Check what kind of update this is
        if episode.is_authenticated() {
            // Authentication successful - Pure P2P style
            println!("🎭 MATRIX UI SUCCESS: User authenticated successfully (Pure P2P)");
            let message = WebSocketMessage {
                message_type: "authentication_successful".to_string(),
                episode_id: Some(episode_id.into()),
                authenticated: Some(true),
                challenge: episode.challenge(),
                session_token: Some("pure_p2p_authenticated".to_string()), // Fake token for frontend compatibility
                comment: None,
                comments: None,
            };
            let _ = self.websocket_tx.send(message);
        } else if !episode.is_authenticated() && episode.challenge().is_some() {
            // Check if this was a session revocation by comparing with previous state
            if let Some(prev_episode) = previous_episode {
                if prev_episode.is_authenticated() {
                    // Previous state was authenticated, now it's not -> session revoked
                    println!("🎭 MATRIX UI SUCCESS: User session revoked (logout completed)");
                    let message = WebSocketMessage {
                        message_type: "session_revoked".to_string(),
                        episode_id: Some(episode_id.into()),
                        authenticated: Some(false),
                        challenge: episode.challenge(),
                        session_token: None,
                        comment: None,
                        comments: None,
                    };
                    let receiver_count = self.websocket_tx.receiver_count();
                    let _ = self.websocket_tx.send(message);
                    println!("📡 Sent session_revoked WebSocket message for episode {} to {} client(s)", episode_id, receiver_count);
                    return; // Don't send challenge_issued message
                }
            }
            
            // Challenge was issued (initial state)
            println!("🎭 MATRIX UI SUCCESS: Authentication challenge issued to user");
            
            // Get the participant-specific challenge if authorization is available
            let participant_challenge = if let Some(participant) = authorization {
                episode.get_challenge_for_participant(&participant)
            } else {
                episode.challenge()
            };
            
            let message = WebSocketMessage {
                message_type: "challenge_issued".to_string(),
                episode_id: Some(episode_id.into()),
                authenticated: Some(false),
                challenge: participant_challenge,
                session_token: None,
                comment: None,
                comments: None,
            };
            let _ = self.websocket_tx.send(message);
        }
    }
    
    fn on_rollback(&self, episode_id: EpisodeId, _episode: &AuthWithCommentsEpisode) {
        println!("🎭 MATRIX UI ERROR: Authentication episode {} rolled back on blockchain", episode_id);
        println!("🔄 Episode {} rolled back on blockchain", episode_id);
    }
}