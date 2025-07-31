// src/api/websocket/auth_handler.rs - Pure WebSocket P2P Implementation
use tokio::sync::{mpsc, broadcast};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};
use std::sync::Arc;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_wrpc_client::prelude::RpcApi;
use kdapp::{
    engine::EpisodeMessage,
    pki::PubKey,
    generator::TransactionGenerator,
};

use crate::core::{episode::SimpleAuth, commands::AuthCommand};
use crate::episode_runner::{AUTH_PATTERN, AUTH_PREFIX};

/// WebSocket message types - much simpler than HTTP!
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsAuthMessage {
    // Participant Peer -> Organizer Peer
    StartAuth {
        public_key: String,
    },
    SignChallenge {
        episode_id: u64,
        signature: String,
        nonce: String,
    },
    RevokeSession {
        episode_id: u64,
        session_token: String,
        signature: String,
    },
    Subscribe {
        episode_id: u64,
    },
    
    // Organizer Peer -> Participant Peer
    Connected {
        organizer_id: String,
        network: String,
    },
    EpisodeCreated {
        episode_id: u64,
        challenge: String, // Send challenge immediately!
    },
    AuthProgress {
        episode_id: u64,
        step: String,
        transaction_id: Option<String>,
    },
    AuthSuccess {
        episode_id: u64,
        session_token: String,
    },
    AuthFailed {
        episode_id: u64,
        reason: String,
    },
    SessionRevoked {
        episode_id: u64,
    },
}

/// Pure WebSocket authentication state
pub struct WsAuthState {
    /// Active episodes (in-memory coordination)
    pub episodes: Arc<tokio::sync::Mutex<HashMap<u64, SimpleAuth>>>,
    /// WebSocket broadcast channel
    pub broadcast_tx: broadcast::Sender<WsAuthMessage>,
    /// Kaspa RPC participant_peer
    pub kaspad_client: Arc<kaspa_wrpc_client::KaspaRpcClient>,
    /// Transaction generator
    pub tx_generator: Arc<TransactionGenerator>,
}

/// WebSocket connection handler
pub struct WsAuthHandler {
    state: Arc<WsAuthState>,
    ws_stream: WebSocketStream<tokio::net::TcpStream>,
    connection_id: String,
    subscriptions: Vec<u64>, // Episode IDs this connection is subscribed to
}

impl WsAuthHandler {
    pub fn new(
        state: Arc<WsAuthState>,
        ws_stream: WebSocketStream<tokio::net::TcpStream>,
    ) -> Self {
        Self {
            state,
            ws_stream,
            connection_id: uuid::Uuid::new_v4().to_string(),
            subscriptions: Vec::new(),
        }
    }
    
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Send welcome message
        let welcome = WsAuthMessage::Connected {
            organizer_id: self.connection_id.clone(),
            network: "testnet-10".to_string(),
        };
        self.send_message(welcome).await?;
        
        // Set up broadcast receiver
        let mut broadcast_rx = self.state.broadcast_tx.subscribe();
        
        loop {
            tokio::select! {
                // Handle incoming messages from participant peer
                Some(msg) = self.ws_stream.next() => {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Err(e) = self.handle_message(&text).await {
                                eprintln!("Message handling error: {}", e);
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Err(e) => {
                            eprintln!("WebSocket error: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }
                
                // Handle broadcast messages
                Ok(broadcast_msg) = broadcast_rx.recv() => {
                    // Only send if subscribed to this episode
                    if let Some(episode_id) = self.get_episode_id(&broadcast_msg) {
                        if self.subscriptions.contains(&episode_id) {
                            let _ = self.send_message(broadcast_msg).await;
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    async fn handle_message(&mut self, text: &str) -> Result<(), Box<dyn std::error::Error>> {
        let msg: WsAuthMessage = serde_json::from_str(text)?;
        
        match msg {
            WsAuthMessage::StartAuth { public_key } => {
                self.handle_start_auth(public_key).await?;
            }
            WsAuthMessage::SignChallenge { episode_id, signature, nonce } => {
                self.handle_sign_challenge(episode_id, signature, nonce).await?;
            }
            WsAuthMessage::RevokeSession { episode_id, session_token, signature } => {
                self.handle_revoke_session(episode_id, session_token, signature).await?;
            }
            WsAuthMessage::Subscribe { episode_id } => {
                if !self.subscriptions.contains(&episode_id) {
                    self.subscriptions.push(episode_id);
                }
            }
            _ => {} // Ignore messages from participant peer we don't handle
        }
        
        Ok(())
    }
    
    async fn handle_start_auth(&mut self, public_key: String) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 WebSocket: Starting authentication for {}", public_key);
        
        // Parse public key
        let participant_pubkey = match hex::decode(&public_key) {
            Ok(bytes) => match secp256k1::PublicKey::from_slice(&bytes) {
                Ok(pk) => PubKey(pk),
                Err(_) => {
                    self.send_message(WsAuthMessage::AuthFailed {
                        episode_id: 0,
                        reason: "Invalid public key".to_string(),
                    }).await?;
                    return Ok(());
                }
            },
            Err(_) => {
                self.send_message(WsAuthMessage::AuthFailed {
                    episode_id: 0,
                    reason: "Invalid hex encoding".to_string(),
                }).await?;
                return Ok(());
            }
        };
        
        // Generate episode ID and challenge
        let episode_id: u64 = rand::random();
        let challenge = format!("auth_{}_{}", 
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            rand::random::<u64>()
        );
        
        // Create in-memory episode with challenge
        let mut episode = SimpleAuth::initialize(
            vec![participant_pubkey],
            &kdapp::episode::PayloadMetadata::default()
        );
        
        // Set challenge immediately
        episode.challenge = Some(challenge.clone());
        
        // Store in memory
        {
            let mut episodes = self.state.episodes.lock().await;
            episodes.insert(episode_id, episode);
        }
        
        // Auto-subscribe this connection
        self.subscriptions.push(episode_id);
        
        // Send response with challenge immediately!
        self.send_message(WsAuthMessage::EpisodeCreated {
            episode_id,
            challenge,
        }).await?;
        
        println!("✅ Episode {} created with immediate challenge", episode_id);
        
        Ok(())
    }
    
    async fn handle_sign_challenge(
        &mut self,
        episode_id: u64,
        signature: String,
        nonce: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("📝 WebSocket: Processing signature for episode {}", episode_id);
        
        // Get episode and verify challenge
        let (participant_pubkey, expected_challenge) = {
            let episodes = self.state.episodes.lock().await;
            match episodes.get(&episode_id) {
                Some(ep) => (ep.owner.unwrap(), ep.challenge.clone().unwrap_or_default()),
                None => {
                    self.send_message(WsAuthMessage::AuthFailed {
                        episode_id,
                        reason: "Episode not found".to_string(),
                    }).await?;
                    return Ok(());
                }
            }
        };
        
        if nonce != expected_challenge {
            self.send_message(WsAuthMessage::AuthFailed {
                episode_id,
                reason: "Challenge mismatch".to_string(),
            }).await?;
            return Ok(());
        }
        
        // Now submit ALL transactions to blockchain
        self.submit_auth_transactions(
            episode_id,
            participant_pubkey,
            signature,
            nonce
        ).await?;
        
        Ok(())
    }
    
    async fn submit_auth_transactions(
        &mut self,
        episode_id: u64,
        participant_pubkey: PubKey,
        signature: String,
        nonce: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Get participant wallet
        let participant_wallet = crate::wallet::get_wallet_for_command("web-participant", None)?;
        let participant_addr = Address::new(
            Prefix::Testnet, 
            Version::PubKey, 
            &participant_wallet.keypair.x_only_public_key().0.serialize()
        );
        
        // Get UTXOs
        let entries = self.state.kaspad_participant_peer
            .get_utxos_by_addresses(vec![participant_addr.clone()])
            .await?;
        
        if entries.is_empty() {
            self.send_message(WsAuthMessage::AuthFailed {
                episode_id,
                reason: format!("No funds. Please fund: {}", participant_addr),
            }).await?;
            return Ok(());
        }
        
        let mut utxo = (
            TransactionOutpoint::from(entries[0].outpoint.clone()),
            UtxoEntry::from(entries[0].utxo_entry.clone())
        );
        
        // Transaction 1: NewEpisode
        self.send_progress(episode_id, "Submitting NewEpisode transaction", None).await?;
        
        let new_episode = EpisodeMessage::<SimpleAuth>::NewEpisode { 
            episode_id: episode_id as u32, 
            participants: vec![participant_pubkey] 
        };
        
        let tx1 = self.state.tx_generator.build_command_transaction(
            utxo.clone(), &participant_addr, &new_episode, 5000
        );
        
        self.state.kaspad_participant_peer
            .submit_transaction(tx1.as_ref().into(), false)
            .await?;
        
        let tx1_id = tx1.id().to_string();
        self.send_progress(episode_id, "NewEpisode submitted", Some(tx1_id)).await?;
        utxo = kdapp::generator::get_first_output_utxo(&tx1);
        
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
        // Transaction 2: RequestChallenge
        self.send_progress(episode_id, "Submitting RequestChallenge transaction", None).await?;
        
        let request_challenge = EpisodeMessage::<SimpleAuth>::new_signed_command(
            episode_id as u32,
            AuthCommand::RequestChallenge,
            participant_wallet.keypair.secret_key(),
            participant_pubkey
        );
        
        let tx2 = self.state.tx_generator.build_command_transaction(
            utxo.clone(), &participant_addr, &request_challenge, 5000
        );
        
        self.state.kaspad_participant_peer
            .submit_transaction(tx2.as_ref().into(), false)
            .await?;
        
        let tx2_id = tx2.id().to_string();
        self.send_progress(episode_id, "RequestChallenge submitted", Some(tx2_id)).await?;
        utxo = kdapp::generator::get_first_output_utxo(&tx2);
        
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
        // Transaction 3: SubmitResponse
        self.send_progress(episode_id, "Submitting authentication response", None).await?;
        
        let submit_response = EpisodeMessage::<SimpleAuth>::new_signed_command(
            episode_id as u32,
            AuthCommand::SubmitResponse { signature, nonce },
            participant_wallet.keypair.secret_key(),
            participant_pubkey
        );
        
        let tx3 = self.state.tx_generator.build_command_transaction(
            utxo, &participant_addr, &submit_response, 5000
        );
        
        self.state.kaspad_participant_peer
            .submit_transaction(tx3.as_ref().into(), false)
            .await?;
        
        let tx3_id = tx3.id().to_string();
        self.send_progress(episode_id, "Authentication submitted", Some(tx3_id)).await?;
        
        // Generate session token
        let session_token = format!("sess_{}", rand::random::<u64>());
        
        // Update in-memory state
        {
            let mut episodes = self.state.episodes.lock().await;
            if let Some(episode) = episodes.get_mut(&episode_id) {
                episode.is_authenticated = true;
                episode.session_token = Some(session_token.clone());
            }
        }
        
        // Broadcast success
        let _ = self.state.broadcast_tx.send(WsAuthMessage::AuthSuccess {
            episode_id,
            session_token: session_token.clone(),
        });
        
        Ok(())
    }
    
    async fn handle_revoke_session(
        &mut self,
        episode_id: u64,
        session_token: String,
        signature: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Similar implementation for session revocation
        // ... (abbreviated for space)
        
        let _ = self.state.broadcast_tx.send(WsAuthMessage::SessionRevoked {
            episode_id,
        });
        
        Ok(())
    }
    
    async fn send_message(&mut self, msg: WsAuthMessage) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string(&msg)?;
        self.ws_stream.send(Message::Text(json)).await?;
        Ok(())
    }
    
    async fn send_progress(
        &mut self,
        episode_id: u64,
        step: &str,
        transaction_id: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(WsAuthMessage::AuthProgress {
            episode_id,
            step: step.to_string(),
            transaction_id,
        }).await
    }
    
    fn get_episode_id(&self, msg: &WsAuthMessage) -> Option<u64> {
        match msg {
            WsAuthMessage::EpisodeCreated { episode_id, .. } |
            WsAuthMessage::AuthProgress { episode_id, .. } |
            WsAuthMessage::AuthSuccess { episode_id, .. } |
            WsAuthMessage::AuthFailed { episode_id, .. } |
            WsAuthMessage::SessionRevoked { episode_id, .. } => Some(*episode_id),
            _ => None,
        }
    }
}

/// Start WebSocket authentication organizer peer
pub async fn start_ws_auth_organizer_peer(
    keypair: secp256k1::Keypair,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Pure WebSocket Authentication Organizer Peer on port {}", port);
    
    // Connect to Kaspa
    let network = kaspa_consensus_core::network::NetworkId::with_suffix(
        kaspa_consensus_core::network::NetworkType::Testnet, 
        10
    );
    let kaspad_participant_peer = Arc::new(kdapp::proxy::connect_participant_peer(network, None).await?);
    
    // Create transaction generator
    let tx_generator = Arc::new(TransactionGenerator::new(
        keypair,
        AUTH_PATTERN,
        AUTH_PREFIX,
    ));
    
    // Create broadcast channel
    let (broadcast_tx, _) = broadcast::channel(1000);
    
    // Create shared state
    let state = Arc::new(WsAuthState {
        episodes: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        broadcast_tx,
        kaspad_participant_peer,
        tx_generator,
    });
    
    // Start WebSocket organizer peer
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    println!("✅ WebSocket organizer peer listening on ws://{}", addr);
    println!("🔗 Connected to Kaspa testnet-10");
    println!("💡 Pure P2P: Each participant funds their own transactions");
    
    while let Ok((stream, addr)) = listener.accept().await {
        println!("🔗 New WebSocket connection from {}", addr);
        
        let ws_stream = tokio_tungstenite::accept_async(stream).await?;
        let handler = WsAuthHandler::new(state.clone(), ws_stream);
        
        tokio::spawn(async move {
            if let Err(e) = handler.run().await {
                eprintln!("WebSocket handler error: {}", e);
            }
        });
    }
    
    Ok(())
}