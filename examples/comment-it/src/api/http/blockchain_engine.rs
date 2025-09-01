// src/api/http/blockchain_engine.rs
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kdapp::{
    engine::Engine,
    episode::{EpisodeEventHandler, EpisodeId},
    generator::TransactionGenerator,
    proxy::connect_client,
};
use secp256k1::Keypair;
use std::collections::{HashMap, HashSet};
use std::sync::{atomic::AtomicBool, mpsc, Arc};
use tokio::sync::broadcast;

use crate::api::http::state::{PeerState, SharedEpisodeState, WebSocketMessage};
use crate::core::{AuthWithCommentsEpisode, UnifiedCommand};
use crate::episode_runner::{AUTH_PATTERN, AUTH_PREFIX};
use kaspa_consensus_core::Hash as KaspaHash;
use kaspa_wrpc_client::prelude::RpcApi;
use std::sync::Mutex as StdMutex;

/// The main HTTP coordination peer that runs a real kdapp engine
pub struct AuthHttpPeer {
    pub peer_state: PeerState,
    pub network: NetworkId,
    pub exit_signal: Arc<AtomicBool>,
    pub engine_sender: StdMutex<Option<std::sync::mpsc::Sender<kdapp::engine::EngineMsg>>>,
}

impl AuthHttpPeer {
    pub async fn new(
        peer_keypair: Keypair,
        websocket_tx: broadcast::Sender<WebSocketMessage>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let network = NetworkId::with_suffix(NetworkType::Testnet, 10);

        let transaction_generator = Arc::new(TransactionGenerator::new(peer_keypair, AUTH_PATTERN, AUTH_PREFIX));

        // Create shared episode state that both engine and HTTP coordination peer can access
        let blockchain_episodes = Arc::new(std::sync::Mutex::new(HashMap::new()));

        // Create kaspad client for transaction submission
        let kaspad_client = match connect_client(network, None).await {
            Ok(client) => {
                println!("✅ Connected to Kaspa node for transaction submission");
                Some(Arc::new(client))
            }
            Err(e) => {
                println!("⚠️ Failed to connect to Kaspa node: {e}");
                println!("📋 Transactions will be created but not submitted");
                None
            }
        };

        let peer_state = PeerState {
            episodes: Arc::new(std::sync::Mutex::new(HashMap::new())), // Legacy
            blockchain_episodes: blockchain_episodes.clone(),          // NEW - real blockchain state
            websocket_tx,
            peer_keypair,
            transaction_generator,
            kaspad_client,                                                     // NEW - for actual transaction submission
            auth_http_peer: None,                                              // Will be set after AuthHttpPeer is created
            pending_requests: Arc::new(std::sync::Mutex::new(HashSet::new())), // NEW - request deduplication
            used_utxos: Arc::new(std::sync::Mutex::new(HashSet::new())),       // NEW - UTXO tracking
            utxo_cache: kdapp::utils::utxo_cache::UtxoCache::new(750),         // NEW - shared short-lived UTXO cache
        };

        let exit_signal = Arc::new(AtomicBool::new(false));

        // Build the peer; organizer code will set auth_http_peer reference
        Ok(AuthHttpPeer { peer_state, network, exit_signal, engine_sender: StdMutex::new(None) })
    }

    /// Start the blockchain listener - this makes HTTP coordination peer a real kdapp node!
    pub async fn start_blockchain_listener(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel();
        // Expose the engine sender for rehydration
        {
            let mut guard = self.engine_sender.lock().unwrap();
            *guard = Some(tx.clone());
        }

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

        // Optional: Rehydrate known episodes from kdapp-indexer so commands for old episodes don't get dropped after restart
        if let Err(e) = self.rehydrate_from_indexer().await {
            println!("⚠️ Rehydrate from indexer failed: {e}");
        }

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
        println!("🔍 Querying blockchain episode state for episode {episode_id}");

        match self.peer_state.blockchain_episodes.lock() {
            Ok(episodes) => {
                if let Some(episode) = episodes.get(&(episode_id as u64)) {
                    println!("✅ Found episode {episode_id} in blockchain state");
                    Some(episode.clone())
                } else {
                    println!("⚠️ Episode {episode_id} not found in blockchain state");
                    None
                }
            }
            Err(e) => {
                println!("❌ Failed to lock blockchain episodes: {e}");
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
                kdapp::engine::EpisodeMessage::SignedCommand { pubkey, .. } => *pubkey,
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
                &participant_pubkey.0.serialize()[1..], // Remove compression byte for address
            );

            println!("🎯 Using REAL participant address: {participant_addr}");
            println!("🔑 Participant pubkey: {}", hex::encode(participant_pubkey.0.serialize()));

            // Get UTXOs for participant (with short-lived cache)
            let entries = self.peer_state.utxo_cache.get(kaspad, &participant_addr).await?;
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
                            kaspa_consensus_core::tx::TransactionOutpoint::from(entry.outpoint),
                            kaspa_consensus_core::tx::UtxoEntry::from(entry.utxo_entry.clone()),
                            utxo_id,
                        ));
                        break;
                    }
                }
            }

            let (outpoint, utxo_entry, utxo_id) =
                selected_utxo.ok_or("No unused UTXOs available. Please wait for previous transactions to confirm.")?;
            let utxo = (outpoint, utxo_entry);

            // Mark this UTXO as used temporarily
            {
                let mut used_utxos = self.peer_state.used_utxos.lock().unwrap();
                used_utxos.insert(utxo_id.clone());
                println!("🔒 Reserved UTXO: {utxo_id}");
            }

            // Build and submit transaction using the transaction generator
            let tx = self.peer_state.transaction_generator.build_command_transaction(
                utxo,
                &participant_addr,
                &episode_message,
                5000, // fee
            );

            // Submit to blockchain
            let submit_result = kaspad.submit_transaction(tx.as_ref().into(), false).await;

            match submit_result {
                Ok(_) => {
                    let tx_id = tx.id().to_string();
                    println!("✅ Transaction {tx_id} submitted to blockchain successfully!");

                    // 🔧 UTXO FIX: Schedule UTXO cleanup after successful submission
                    let used_utxos_cleanup = self.peer_state.used_utxos.clone();
                    let utxo_id_cleanup = utxo_id.clone();
                    tokio::spawn(async move {
                        // Wait 10 seconds then remove from used set (transaction should be confirmed by then)
                        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                        if let Ok(mut used_utxos) = used_utxos_cleanup.lock() {
                            used_utxos.remove(&utxo_id_cleanup);
                            println!("🔓 Released UTXO: {utxo_id_cleanup}");
                        }
                    });

                    Ok(tx_id)
                }
                Err(e) => {
                    // 🔧 UTXO FIX: Release UTXO immediately if transaction fails
                    {
                        let mut used_utxos = self.peer_state.used_utxos.lock().unwrap();
                        used_utxos.remove(&utxo_id);
                        println!("🔓 Released UTXO due to error: {utxo_id}");
                    }
                    Err(e.into())
                }
            }
        } else {
            Err("Kaspad client not available for transaction submission.".into())
        }
    }
}

impl AuthHttpPeer {
    async fn rehydrate_from_indexer(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Only attempt if an engine sender is available
        let sender = match self.engine_sender.lock().unwrap().as_ref().cloned() {
            Some(s) => s,
            None => return Ok(()),
        };
        let base = std::env::var("INDEXER_URL").unwrap_or_else(|_| "http://127.0.0.1:8090".to_string());
        let url = format!("{}/index/recent?limit=200", base.trim_end_matches('/'));
        let client = reqwest::Client::new();
        let resp = client.get(url).send().await?;
        if !resp.status().is_success() {
            return Ok(());
        }
        let json: serde_json::Value = resp.json().await?;
        let episodes = json.get("episodes").and_then(|v| v.as_array()).cloned().unwrap_or_default();

        // Fetch current DAA score to prevent immediate lifetime filter purge
        let current_daa = match &self.peer_state.kaspad_client {
            Some(k) => match k.get_block_dag_info().await {
                Ok(info) => info.virtual_daa_score,
                Err(_) => 0,
            },
            None => 0,
        };

        // Build a set of already-loaded episodes to avoid duplicates
        let existing: std::collections::HashSet<u64> = match self.peer_state.blockchain_episodes.lock() {
            Ok(map) => map.keys().cloned().collect(),
            Err(_) => Default::default(),
        };

        let mut count = 0usize;
        for ep in episodes {
            let id = match ep.get("episode_id").and_then(|v| v.as_u64()) {
                Some(v) => v,
                None => continue,
            };
            if existing.contains(&id) {
                continue;
            }
            // Parse creator pubkey if present; otherwise skip
            let creator = match ep.get("creator_pubkey").and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => s,
                _ => continue,
            };
            // Try to parse as hex; also accept wrapped PublicKey(hex) format
            let hex_str =
                if creator.starts_with("PublicKey(") && creator.ends_with(")") { &creator[10..creator.len() - 1] } else { creator };
            let pk_bytes = match hex::decode(hex_str) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let pk = match secp256k1::PublicKey::from_slice(&pk_bytes) {
                Ok(p) => kdapp::pki::PubKey(p),
                Err(_) => continue,
            };
            let id_u32: u32 = match id.try_into() {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Serialize a synthetic NewEpisode payload
            let action = kdapp::engine::EpisodeMessage::<crate::core::AuthWithCommentsEpisode>::NewEpisode {
                episode_id: id_u32,
                participants: vec![pk],
            };
            let payload = borsh::to_vec(&action).unwrap_or_default();
            // Minimal metadata — use a fresh accepting_daa to avoid immediate filtering
            let accepting_hash = KaspaHash::from_bytes([0u8; 32]);
            let accepting_daa = if current_daa > 0 { current_daa } else { 1 };
            // Use created_at if present for a nicer timestamp
            let accepting_time = ep.get("created_at").and_then(|v| v.as_u64()).unwrap_or(0);
            let msg = kdapp::engine::EngineMsg::BlkAccepted {
                accepting_hash,
                accepting_daa,
                accepting_time,
                associated_txs: vec![(KaspaHash::from_bytes([0u8; 32]), payload, None)],
            };
            let _ = sender.send(msg);
            count += 1;
        }
        if count > 0 {
            println!("🔄 Rehydrated {count} episode(s) from kdapp-indexer");
        }
        Ok(())
    }
}

/// Episode event handler that broadcasts updates to WebSocket clients
pub struct HttpAuthHandler {
    pub websocket_tx: broadcast::Sender<WebSocketMessage>,
    pub blockchain_episodes: SharedEpisodeState,
}

impl EpisodeEventHandler<AuthWithCommentsEpisode> for HttpAuthHandler {
    fn on_initialize(&self, episode_id: EpisodeId, episode: &AuthWithCommentsEpisode) {
        println!("🎭 MATRIX UI SUCCESS: Auth episode {episode_id} initialized on blockchain");
        println!("🎬 Episode {episode_id} initialized on blockchain");

        // Store episode in shared blockchain state
        if let Ok(mut episodes) = self.blockchain_episodes.lock() {
            episodes.insert(episode_id.into(), episode.clone());
            println!("✅ Stored episode {episode_id} in blockchain state");
        } else {
            println!("❌ Failed to store episode {episode_id} in blockchain state");
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
        println!("⚡ Episode {episode_id} updated on blockchain");
        println!("🔍 DEBUG: on_command called for episode {episode_id} with command: {cmd:?}");

        // Read previous state BEFORE updating (for session revocation detection and comment detection)
        let previous_episode =
            if let Ok(episodes) = self.blockchain_episodes.lock() { episodes.get(&(episode_id as u64)).cloned() } else { None };

        // Update episode in shared blockchain state
        if let Ok(mut episodes) = self.blockchain_episodes.lock() {
            episodes.insert(episode_id.into(), episode.clone());
            println!("✅ Updated episode {episode_id} in blockchain state");
        } else {
            println!("❌ Failed to update episode {episode_id} in blockchain state");
        }

        // 🚀 CRITICAL: Check for new comments and broadcast them real-time!
        if let UnifiedCommand::SubmitComment { text, session_token: _ } = cmd {
            println!("💬 NEW COMMENT detected on blockchain for episode {episode_id}");
            println!("📝 Comment text: \"{text}\"");

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
                        timestamp: latest_comment.timestamp,
                    }),
                    comments: None,
                };

                let receiver_count = self.websocket_tx.receiver_count();
                let _ = self.websocket_tx.send(message);
                println!("📡 NEW COMMENT broadcasted to {receiver_count} connected peer(s)! 🎉");
            }
            return; // Don't process as auth command
        }

        // First, detect session revocation regardless of challenge presence
        if let Some(prev_episode) = previous_episode {
            if prev_episode.is_authenticated() && !episode.is_authenticated() {
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
                println!("📡 Sent session_revoked WebSocket message for episode {episode_id} to {receiver_count} client(s)");
                return; // Done handling revocation update
            }
        }

        // Otherwise, check what kind of update this is
        if episode.is_authenticated() {
            // Authentication successful - Pure P2P style
            println!("🎭 MATRIX UI SUCCESS: User authenticated successfully (Pure P2P)");
            // Compute deterministic session handle if we know which pubkey authenticated
            let session_handle = authorization.map(|pk| deterministic_handle(episode_id.into(), &pk));
            let message = WebSocketMessage {
                message_type: "authentication_successful".to_string(),
                episode_id: Some(episode_id.into()),
                authenticated: Some(true),
                challenge: episode.challenge(),
                session_token: session_handle,
                comment: None,
                comments: None,
            };
            let _ = self.websocket_tx.send(message);
        } else if !episode.is_authenticated() && episode.challenge().is_some() {
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
        println!("🎭 MATRIX UI ERROR: Authentication episode {episode_id} rolled back on blockchain");
        println!("🔄 Episode {episode_id} rolled back on blockchain");
    }
}

fn deterministic_handle(episode_id: u64, pubkey: &kdapp::pki::PubKey) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"KDAPP/COMMENT-IT/SESSION");
    hasher.update(episode_id.to_be_bytes());
    hasher.update(pubkey.0.serialize());
    let out = hasher.finalize();
    hex::encode(out)
}
