// src/api/http/handlers/verify.rs
use axum::{extract::State, http::StatusCode, response::Json};

use crate::api::http::{
    state::PeerState,
    types::{VerifyRequest, VerifyResponse},
};
use crate::core::AuthWithCommentsEpisode;
use kdapp::{engine::EpisodeMessage, pki::PubKey};
use std::collections::HashSet;
use std::sync::Arc;

pub async fn verify_auth(State(state): State<PeerState>, Json(req): Json<VerifyRequest>) -> Result<Json<VerifyResponse>, StatusCode> {
    println!("🎭 MATRIX UI ACTION: User submitted authentication signature");
    println!("🔍 DEBUG: Verify request received - episode_id: {}, nonce: {}", req.episode_id, req.nonce);
    println!("🔍 DEBUG: Signature length: {}", req.signature.len());
    println!("📤 Sending SubmitResponse command to blockchain...");

    // Parse episode_id from request (u64)
    let episode_id: u64 = req.episode_id;

    // 🚨 CRITICAL: Request-level deduplication to prevent race conditions
    let request_key = format!("verify_{episode_id}");
    {
        let mut pending = state.pending_requests.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if pending.contains(&request_key) {
            println!("🔄 Duplicate verify request for episode {episode_id} blocked - request already in progress");
            return Ok(Json(VerifyResponse {
                episode_id,
                authenticated: false,
                status: "request_in_progress".to_string(),
                transaction_id: None,
            }));
        }
        pending.insert(request_key.clone());
    }

    // Ensure we remove the request key when done (RAII-style cleanup)
    let _cleanup_guard = RequestCleanupGuard { pending_requests: state.pending_requests.clone(), request_key: request_key.clone() };

    // Read episode for quick state checks (not for pubkey)
    let episode = match state.blockchain_episodes.lock() {
        Ok(episodes) => episodes.get(&episode_id).cloned(),
        Err(e) => {
            println!("❌ Failed to lock blockchain episodes: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    if let Some(ep) = &episode {
        // If episode already has authenticated participants, skip duplicate submissions
        if ep.is_authenticated() {
            println!("🔄 Episode {episode_id} already authenticated - blocking duplicate transaction submission");
            return Ok(Json(VerifyResponse {
                episode_id,
                authenticated: true,
                status: "already_authenticated".to_string(),
                transaction_id: None,
            }));
        }
    } else {
        println!("⚠️ Episode {episode_id} not found in blockchain state (rehydrating or pending)");
        // Continue anyway — engine will initialize on first commands
    }

    // 🎯 TRUE P2P: Participant funds their own transactions (like CLI)
    let participant_wallet =
        crate::wallet::get_wallet_for_command("web-participant", None).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let participant_secret_key = participant_wallet.keypair.secret_key();
    let participant_pubkey = PubKey(participant_wallet.keypair.public_key());

    // Create participant's Kaspa address for transaction funding (True P2P!)
    let _participant_addr = kaspa_addresses::Address::new(
        kaspa_addresses::Prefix::Testnet,
        kaspa_addresses::Version::PubKey,
        &participant_wallet.keypair.x_only_public_key().0.serialize(),
    );

    // Create SubmitResponse command (exactly like CLI)
    let auth_command = crate::core::UnifiedCommand::SubmitResponse { signature: req.signature.clone(), nonce: req.nonce.clone() };

    // Convert episode_id from u64 to u32 for EpisodeMessage (kdapp framework requirement)
    let episode_id_u32 = match episode_id.try_into() {
        Ok(id) => id,
        Err(_) => {
            println!("❌ Episode ID {episode_id} is too large to fit in u32");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let step = EpisodeMessage::<AuthWithCommentsEpisode>::new_signed_command(
        episode_id_u32,
        auth_command,
        participant_secret_key, // 🚨 CRITICAL: Participant signs for episode authorization!
        participant_pubkey,     // Use participant's public key for episode authorization
    );

    // Submit transaction to blockchain via AuthHttpPeer
    println!("📤 Submitting SubmitResponse transaction to Kaspa blockchain via AuthHttpPeer...");
    let submission_result = match state.auth_http_peer.as_ref().unwrap().submit_episode_message_transaction(step).await {
        Ok(tx_id) => {
            println!("✅ MATRIX UI SUCCESS: Authentication signature submitted - Transaction {tx_id}");
            println!("📊 Transaction submitted to Kaspa blockchain - organizer peer will detect and respond");
            (tx_id, "submit_response_submitted".to_string())
        }
        Err(e) => {
            println!("❌ MATRIX UI ERROR: Authentication signature submission failed - {e}");
            ("error".to_string(), "submit_response_failed".to_string())
        }
    };

    let (transaction_id, status) = submission_result;

    Ok(Json(VerifyResponse {
        episode_id,
        authenticated: false, // Will be updated by blockchain when processed
        status,
        transaction_id: Some(transaction_id),
    }))
}

/// RAII cleanup guard to remove pending request when function exits
struct RequestCleanupGuard {
    pending_requests: Arc<std::sync::Mutex<HashSet<String>>>,
    request_key: String,
}

impl Drop for RequestCleanupGuard {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.pending_requests.lock() {
            pending.remove(&self.request_key);
            println!("🧹 Cleaned up pending request: {}", self.request_key);
        }
    }
}
