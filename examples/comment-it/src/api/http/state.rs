// src/api/http/state.rs
use crate::core::AuthWithCommentsEpisode;
use kaspa_wrpc_client::KaspaRpcClient;
use kdapp::generator::TransactionGenerator;
use secp256k1::Keypair;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

// Real blockchain-based episode state (not the old fake HashMap approach)
pub type SharedEpisodeState = Arc<Mutex<HashMap<u64, AuthWithCommentsEpisode>>>;

#[derive(Clone)]
pub struct EpisodeState {
    pub public_key: String,
    pub authenticated: bool,
    pub status: String,
}

#[derive(Clone)]
pub struct PeerState {
    pub episodes: Arc<Mutex<HashMap<u64, EpisodeState>>>, // Legacy - will remove
    pub blockchain_episodes: SharedEpisodeState,          // NEW - real blockchain state
    pub websocket_tx: broadcast::Sender<WebSocketMessage>,
    pub peer_keypair: Keypair,
    pub transaction_generator: Arc<TransactionGenerator>,
    pub kaspad_client: Option<Arc<KaspaRpcClient>>, // NEW - for transaction submission
    pub auth_http_peer: Option<Arc<crate::api::http::blockchain_engine::AuthHttpPeer>>, // Reference to the main peer
    pub pending_requests: Arc<Mutex<HashSet<String>>>, // NEW - Track pending requests by operation+episode_id
    pub used_utxos: Arc<Mutex<HashSet<String>>>,    // NEW - Track used UTXOs to prevent double-spending
    // Short-lived UTXO cache provided by kdapp utils
    pub utxo_cache: std::sync::Arc<kdapp::utils::utxo_cache::UtxoCache>,
}

// WebSocket message for real-time blockchain updates
#[derive(Clone, Debug, serde::Serialize)]
pub struct WebSocketMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub episode_id: Option<u64>,
    pub authenticated: Option<bool>,
    pub challenge: Option<String>,
    pub session_token: Option<String>,
    // Comment-related fields
    pub comment: Option<crate::api::http::types::CommentData>,
    pub comments: Option<Vec<crate::api::http::types::CommentData>>,
}
