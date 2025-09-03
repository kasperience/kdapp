// src/api/http/handlers/revoke.rs
use axum::{extract::State, http::StatusCode, response::Json};
use kaspa_addresses::{Address, Prefix, Version};

use crate::api::http::{
    state::PeerState,
    types::{RevokeSessionRequest, RevokeSessionResponse},
};
use crate::core::AuthWithCommentsEpisode;
use kdapp::{engine::EpisodeMessage, pki::PubKey};

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
            println!("🔍 DEBUG: Looking for episode {} in {} total episodes", episode_id, episodes.len());
            for (id, ep) in episodes.iter() {
                println!("🔍 DEBUG: Found episode {} with owner: {:?}, session_token: {:?}", id, ep.owner(), ep.session_token());
            }
            episodes.get(&episode_id).cloned()
        }
        Err(e) => {
            println!("❌ Failed to lock blockchain episodes: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let (participant_pubkey, _current_session_token) = match episode {
        Some(ref ep) => {
            let pubkey = ep.owner().unwrap_or_else(|| {
                println!("❌ Episode has no owner public key");
                // This shouldn't happen, but let's continue anyway
                PubKey(secp256k1::PublicKey::from_slice(&[2; 33]).unwrap())
            });
            (pubkey, ep.session_token())
        }
        None => {
            println!("❌ Episode {episode_id} not found in blockchain state");
            return Err(StatusCode::NOT_FOUND);
        }
    };

    // Pure P2P mode: no server-side session token storage. We rely on engine checks:
    // - participant must be authenticated
    // - signature must verify against provided token
    // Therefore, do not enforce a stored session token here.

    // 🎯 TRUE P2P: Participant funds their own session revocation transaction
    let participant_wallet = crate::wallet::get_wallet_for_command("web-participant", None).map_err(|e| {
        println!("❌ Failed to load web-participant wallet: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let participant_secret_key = participant_wallet.keypair.secret_key();

    // Create participant's Kaspa address for transaction funding (True P2P!)
    let _participant_addr =
        Address::new(Prefix::Testnet, Version::PubKey, &participant_wallet.keypair.x_only_public_key().0.serialize());

    // Create RevokeSession command
    let auth_command =
        crate::core::UnifiedCommand::RevokeSession { session_token: req.session_token.clone(), signature: req.signature.clone() };

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
    println!("📤 Submitting RevokeSession transaction to Kaspa blockchain via AuthHttpPeer...");
    let submission_result = match state.auth_http_peer.as_ref().unwrap().submit_episode_message_transaction(step).await {
        Ok(tx_id) => {
            println!("✅ MATRIX UI SUCCESS: Session revocation submitted - Transaction {tx_id}");
            println!("📊 Transaction submitted to Kaspa blockchain - organizer peer will detect and respond");
            (tx_id, "session_revocation_submitted".to_string())
        }
        Err(e) => {
            println!("❌ MATRIX UI ERROR: Session revocation failed - {e}");
            ("error".to_string(), "session_revocation_failed".to_string())
        }
    };

    let (transaction_id, status) = submission_result;

    Ok(Json(RevokeSessionResponse { episode_id, transaction_id, status }))
}
