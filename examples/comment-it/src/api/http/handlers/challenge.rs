// src/api/http/handlers/challenge.rs
use axum::extract::rejection::JsonRejection;
use axum::{extract::State, http::StatusCode, response::Json};

use crate::api::http::{
    state::PeerState,
    types::{ChallengeRequest, ChallengeResponse},
};
use crate::core::AuthWithCommentsEpisode;
use kdapp::{engine::EpisodeMessage, pki::PubKey};
use std::collections::HashSet;
use std::sync::Arc;

pub async fn request_challenge(
    State(state): State<PeerState>,
    payload: Result<Json<ChallengeRequest>, JsonRejection>,
) -> Result<Json<ChallengeResponse>, StatusCode> {
    println!("🎭 MATRIX UI ACTION: User requested authentication challenge");
    println!("📨 Sending RequestChallenge command to blockchain...");

    // Be tolerant to early/empty/invalid JSON bodies from the UI. If JSON parsing fails,
    // return 200 with a neutral status so the UI doesn't show errors; the websocket will
    // deliver the actual challenge shortly after episode creation.
    let req = match payload {
        Ok(json) => json.0,
        Err(err) => {
            println!("⚠️ Tolerating invalid challenge request payload: {err}");
            return Ok(Json(ChallengeResponse {
                episode_id: 0,
                // Return a non-empty placeholder to avoid frontend treating it as an error immediately
                nonce: "pending".to_string(),
                transaction_id: None,
                status: "request_in_progress".to_string(),
            }));
        }
    };

    // 🚨 CRITICAL: Request-level deduplication to prevent race conditions
    let request_key = format!("challenge_{}", req.episode_id);
    {
        let mut pending = state.pending_requests.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if pending.contains(&request_key) {
            println!("🔄 Duplicate challenge request for episode {} blocked - request already in progress", req.episode_id);
            return Ok(Json(ChallengeResponse {
                episode_id: req.episode_id,
                nonce: "request_in_progress".to_string(),
                transaction_id: None,
                status: "request_in_progress".to_string(),
            }));
        }
        pending.insert(request_key.clone());
    }

    // Resolve participant public key: prefer request, else episode owner
    let participant_pubkey = if let Some(pk_hex) = &req.public_key {
        match hex::decode(pk_hex) {
            Ok(bytes) => match secp256k1::PublicKey::from_slice(&bytes) {
                Ok(pk) => PubKey(pk),
                Err(e) => {
                    println!("❌ Public key parsing failed: {e}");
                    return Err(StatusCode::BAD_REQUEST);
                }
            },
            Err(e) => {
                println!("❌ Hex decode failed: {e}");
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    } else {
        // Fallback: use episode owner from blockchain state
        let owner = match state.blockchain_episodes.lock() {
            Ok(episodes) => episodes.get(&req.episode_id).and_then(|ep| ep.owner()),
            Err(_) => None,
        };
        match owner {
            Some(pk) => pk,
            None => {
                println!("❌ Missing public_key and episode owner unknown for episode {}", req.episode_id);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    };

    // Ensure we remove the request key when done (RAII-style cleanup)
    let _cleanup_guard = RequestCleanupGuard { pending_requests: state.pending_requests.clone(), request_key: request_key.clone() };

    // 🎯 TRUE P2P: Participant funds their own transactions (like CLI)
    let participant_wallet =
        crate::wallet::get_wallet_for_command("web-participant", None).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let participant_secret_key = participant_wallet.keypair.secret_key();

    // Create participant's Kaspa address for transaction funding (True P2P!)
    let _participant_addr = kaspa_addresses::Address::new(
        kaspa_addresses::Prefix::Testnet,
        kaspa_addresses::Version::PubKey,
        &participant_wallet.keypair.x_only_public_key().0.serialize(),
    );

    // Create RequestChallenge command signed by PARTICIPANT (exactly like CLI)
    let auth_command = crate::core::UnifiedCommand::RequestChallenge;
    let step = EpisodeMessage::<AuthWithCommentsEpisode>::new_signed_command(
        req.episode_id.try_into().unwrap(),
        auth_command,
        participant_secret_key, // 🚨 CRITICAL: Participant signs their own commands!
        participant_pubkey,
    );

    // Submit transaction to blockchain via AuthHttpPeer
    println!("📤 Submitting RequestChallenge transaction to Kaspa blockchain via AuthHttpPeer...");
    let submission_result = match state.auth_http_peer.as_ref().unwrap().submit_episode_message_transaction(step).await {
        Ok(tx_id) => {
            println!("✅ MATRIX UI SUCCESS: Challenge request submitted - Transaction {tx_id}");
            println!("⏳ Organizer peer will generate challenge and update episode on blockchain");
            (tx_id, "request_challenge_submitted".to_string())
        }
        Err(e) => {
            println!("❌ MATRIX UI ERROR: Challenge request failed - {e}");
            ("error".to_string(), "request_challenge_failed".to_string())
        }
    };

    let (transaction_id, status) = submission_result;

    // Wait for blockchain to process RequestChallenge and generate challenge
    let mut challenge_nonce = String::new();
    let mut attempts = 0;
    let max_attempts = 30; // 6 second timeout (30 attempts * 200ms) - should be fast now

    while challenge_nonce.is_empty() && attempts < max_attempts {
        if let Some(episode) = state.blockchain_episodes.lock().unwrap().get(&req.episode_id) {
            if let Some(challenge) = episode.get_challenge_for_participant(&participant_pubkey) {
                challenge_nonce = challenge.clone();
                println!("✅ Challenge generated by blockchain: {challenge_nonce}");
                break;
            }
        }

        attempts += 1;
        if attempts % 10 == 0 {
            println!("⏳ Waiting for blockchain to generate challenge... attempt {attempts}/{max_attempts}");
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    if challenge_nonce.is_empty() {
        println!("❌ Timeout waiting for blockchain challenge generation");
        return Err(StatusCode::REQUEST_TIMEOUT);
    }

    // Clean up pending request
    {
        let mut pending = state.pending_requests.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        pending.remove(&request_key);
        println!("🧹 Cleaned up pending request: {request_key}");
    }

    Ok(Json(ChallengeResponse { episode_id: req.episode_id, nonce: challenge_nonce, transaction_id: Some(transaction_id), status }))
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
