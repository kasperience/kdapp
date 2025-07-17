// src/api/http/handlers/revoke.rs
use axum::{extract::State, response::Json, http::StatusCode};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_wrpc_client::prelude::RpcApi;
use kdapp::{
    engine::EpisodeMessage,
    pki::PubKey,
};
use crate::api::http::{
    types::{RevokeSessionRequest, RevokeSessionResponse},
    state::PeerState,
};
use crate::core::{AuthWithCommentsEpisode, UnifiedCommand};

pub async fn revoke_session(
    State(state): State<PeerState>,
    Json(req): Json<RevokeSessionRequest>,
) -> Result<Json<RevokeSessionResponse>, StatusCode> {
    println!("🎭 MATRIX UI ACTION: User requested session revocation (logout)");
    println!("🔄 DEBUG: RevokeSession request received - episode_id: {}, session_token: {}", req.episode_id, req.session_token);
    println!("🔍 DEBUG: Signature length: {}", req.signature.len());
    println!("📤 Sending RevokeSession command to blockchain...");
    
    // Parse episode_id from request (u64)
    let episode_id: u64 = req.episode_id;
    
    // Find the participant public key from the episode
    let episode = match state.blockchain_episodes.lock() {
        Ok(episodes) => {
            episodes.get(&episode_id).cloned()
        }
        Err(e) => {
            println!("❌ Failed to lock blockchain episodes: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    
    let (participant_pubkey, current_session_token) = match episode {
        Some(ref ep) => {
            let pubkey = ep.owner.unwrap_or_else(|| {
                println!("❌ Episode has no owner public key");
                // This shouldn't happen, but let's continue anyway
                PubKey(secp256k1::PublicKey::from_slice(&[2; 33]).unwrap())
            });
            (pubkey, ep.session_token.clone())
        },
        None => {
            println!("❌ Episode {} not found in blockchain state", episode_id);
            return Err(StatusCode::NOT_FOUND);
        }
    };
    
    // Verify that the session token matches the current episode session
    if let Some(ref current_token) = current_session_token {
        if req.session_token != *current_token {
            println!("❌ MATRIX UI ERROR: Session token mismatch for logout");
            return Err(StatusCode::BAD_REQUEST);
        }
    } else {
        println!("❌ MATRIX UI ERROR: No active session found for logout");
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // 🎯 TRUE P2P: Participant funds their own transactions (like CLI)
    let participant_wallet = crate::wallet::get_wallet_for_command("web-participant", None)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let participant_secret_key = participant_wallet.keypair.secret_key();
    
    // Create RevokeSession command (exactly like CLI)
    let auth_command = crate::core::UnifiedCommand::RevokeSession {
        session_token: req.session_token.clone(),
        signature: req.signature.clone(),
    };
    
    // Convert episode_id from u64 to u32 for EpisodeMessage (kdapp framework requirement)
    let episode_id_u32 = match episode_id.try_into() {
        Ok(id) => id,
        Err(_) => {
            println!("❌ Episode ID {} is too large to fit in u32", episode_id);
            return Err(StatusCode::BAD_REQUEST);
        }
    };
    
    let step = EpisodeMessage::<AuthWithCommentsEpisode>::new_signed_command(
        episode_id_u32, 
        auth_command, 
        participant_secret_key, // 🚨 CRITICAL: Participant signs for episode authorization!
        participant_pubkey // Use participant's public key for episode authorization
    );
    
    // Submit transaction to blockchain via AuthHttpPeer
    println!("📤 Submitting RevokeSession transaction to Kaspa blockchain via AuthHttpPeer...");
    let submission_result = match state.auth_http_peer.as_ref().unwrap().submit_episode_message_transaction(
        step,
    ).await {
        Ok(tx_id) => {
            println!("✅ RevokeSession transaction {} submitted successfully to blockchain via AuthHttpPeer!", tx_id);
            println!("📊 Session revocation processed by auth organizer peer's kdapp engine");
            tx_id
        }
        Err(e) => {
            println!("❌ RevokeSession submission failed via AuthHttpPeer: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    
    Ok(Json(RevokeSessionResponse {
        episode_id,
        transaction_id: submission_result,
        status: "session_revocation_submitted".to_string(),
    }))
}