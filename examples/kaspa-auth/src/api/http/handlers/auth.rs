// src/api/http/handlers/auth.rs - FIXED VERSION
use axum::{extract::State, response::Json, http::StatusCode};
use kaspa_addresses::{Address, Prefix, Version};
use kdapp::{pki::PubKey, episode::{Episode, PayloadMetadata}};
use rand::Rng;
use crate::api::http::{
    types::{AuthRequest, AuthResponse},
    state::PeerState,
};
use crate::core::episode::SimpleAuth;

/// Start authentication - HTTP coordination only, no blockchain!
pub async fn start_auth(
    State(state): State<PeerState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    println!("🚀 Starting authentication episode (HTTP coordination)...");
    
    // Parse participant's public key
    let participant_pubkey = match hex::decode(&req.public_key) {
        Ok(bytes) => match secp256k1::PublicKey::from_slice(&bytes) {
            Ok(pk) => PubKey(pk),
            Err(_) => return Err(StatusCode::BAD_REQUEST),
        },
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };
    
    // Generate episode ID
    let episode_id: u64 = rand::thread_rng().gen();
    
    // Create participant address for display
    let participant_addr = Address::new(
        Prefix::Testnet, 
        Version::PubKey, 
        &participant_pubkey.0.x_only_public_key().0.serialize()
    );
    
    // Create in-memory episode for coordination
    let metadata = PayloadMetadata { 
        accepting_hash: 0u64.into(), 
        accepting_daa: 0, 
        accepting_time: 0, 
        tx_id: episode_id.into()
    };
    let mut episode = SimpleAuth::initialize(
        vec![participant_pubkey],
        &metadata
    );
    
    // Store in coordination state (NOT blockchain yet!)
    {
        let mut episodes = state.blockchain_episodes.lock().unwrap();
        episodes.insert(episode_id, episode);
    }
    
    println!("✅ Episode {} created for HTTP coordination", episode_id);
    println!("📝 Participant should submit NewEpisode transaction themselves");
    
    Ok(Json(AuthResponse {
        episode_id,
        organizer_public_key: hex::encode(state.peer_keypair.public_key().serialize()),
        participant_kaspa_address: participant_addr.to_string(),
        transaction_id: None, // No transaction - just coordination!
        status: "episode_created_awaiting_blockchain".to_string(),
    }))
}