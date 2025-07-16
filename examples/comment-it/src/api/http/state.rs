// src/api/http/state.rs
use std::sync::{Arc, Mutex};
use std::collections::{HashMap, HashSet};
use tokio::sync::broadcast;
use secp256k1::Keypair;
use kdapp::generator::TransactionGenerator;
use crate::core::episode::SimpleAuth;
use kaspa_wrpc_client::KaspaRpcClient;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_addresses::{Address, Prefix, Version};
use kdapp::engine::EpisodeMessage;
use kdapp::pki::PubKey;
use kaspa_rpc_core::api::rpc::RpcApi;

use crate::core::{AuthCommand, episode::SimpleAuth};
use crate::wallet::KaspaAuthWallet;

// Real blockchain-based episode state (not the old fake HashMap approach)
pub type SharedEpisodeState = Arc<Mutex<HashMap<u64, SimpleAuth>>>;

#[derive(Clone)]
pub struct EpisodeState {
    pub public_key: String,
    pub authenticated: bool,
    pub status: String,
}

#[derive(Clone)]
pub struct PeerState {
    pub episodes: Arc<Mutex<HashMap<u64, EpisodeState>>>,  // Legacy - will remove
    pub blockchain_episodes: SharedEpisodeState,  // NEW - real blockchain state
    pub websocket_tx: broadcast::Sender<WebSocketMessage>,
    pub peer_keypair: Keypair,
    pub transaction_generator: Arc<TransactionGenerator>,
    pub kaspad_client: Option<Arc<KaspaRpcClient>>,  // NEW - for transaction submission
    pub auth_http_peer: Option<Arc<crate::api::http::blockchain_engine::AuthHttpPeer>>, // Reference to the main peer
    pub pending_requests: Arc<Mutex<HashSet<String>>>,  // NEW - Track pending requests by operation+episode_id
}

impl PeerState {
    pub async fn get_participant_wallet(&self) -> Result<KaspaAuthWallet, String> {
        KaspaAuthWallet::load_for_command("participant-peer")
            .map_err(|e| format!("Failed to load participant wallet: {}", e))
    }

    pub async fn submit_command_transaction(
        &self,
        wallet: &KaspaAuthWallet,
        episode_id: u64,
        command: AuthCommand,
    ) -> Result<TransactionId, String> {
        let kaspad = self.kaspad_client.clone().ok_or("Kaspad client not available".to_string())?;

        let participant_pubkey = PubKey(wallet.keypair.x_only_public_key().0.into());
        let msg = EpisodeMessage::<SimpleAuth>::new_signed_command(
            episode_id as u32, 
            command, 
            wallet.keypair.secret_key(), 
            participant_pubkey
        );

        let participant_addr = wallet.get_kaspa_address();

        let entries = kaspad.get_utxos_by_addresses(vec![participant_addr.clone()]).await
            .map_err(|e| format!("UTXO fetch failed: {}", e))?;

        if entries.is_empty() {
            return Err(format!("No UTXOs for participant wallet! Fund: {}", participant_addr));
        }

        let utxo = (kaspa_consensus_core::tx::TransactionOutpoint::from(entries[0].outpoint.clone()),
                    kaspa_consensus_core::tx::UtxoEntry::from(entries[0].utxo_entry.clone()));

        let tx = self.transaction_generator.build_command_transaction(
            utxo, &participant_addr, &msg, 5000
        );

        kaspad.submit_transaction(tx.as_ref().into(), false).await
            .map_err(|e| format!("Submit failed: {}", e))?;

        Ok(tx.id())
    }
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
    pub comment: Option<crate::Comment>,
    pub comments: Option<Vec<crate::Comment>>,
}