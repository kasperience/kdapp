// src/api/http/handlers/verify.rs - Refactored for P2P compliance
use crate::api::http::{
    state::PeerState,
    types::{VerifyRequest, VerifyResponse},
};
use crate::core::commands::AuthCommand;
use axum::http::StatusCode;
use axum::{extract::State, Json};
use kdapp::episode::{Episode, PayloadMetadata};
use secp256k1::ecdsa::Signature;

/// Verify authentication - Organizer Peer performs in-memory verification only.
/// Participant is responsible for submitting blockchain transactions.
pub async fn verify_auth(State(state): State<PeerState>, Json(req): Json<VerifyRequest>) -> Result<Json<VerifyResponse>, StatusCode> {
    println!("🔍 Verifying authentication (in-memory only)...");

    let episode_id: u64 = req.episode_id;

    // Get episode and participant info from in-memory state
    let (participant_pubkey, current_challenge) = {
        let episodes = state.blockchain_episodes.lock().unwrap();
        match episodes.get(&episode_id) {
            Some(ep) => {
                if ep.is_authenticated {
                    return Ok(Json(VerifyResponse {
                        episode_id,
                        authenticated: true,
                        status: "already_authenticated".to_string(),
                        transaction_id: None,
                    }));
                }
                (ep.owner.unwrap(), ep.challenge.clone())
            }
            None => return Err(StatusCode::NOT_FOUND),
        }
    };

    // Verify challenge matches
    if current_challenge.as_ref() != Some(&req.nonce) {
        println!("❌ Challenge mismatch: Expected {:?}, got {}", current_challenge, req.nonce);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Verify the signature locally (in-memory)
    let secp = secp256k1::Secp256k1::new();
    let message = kdapp::pki::to_message(&req.nonce);
    let signature_bytes = hex::decode(&req.signature).map_err(|_| StatusCode::BAD_REQUEST)?;
    let signature = Signature::from_der(&signature_bytes).map_err(|_| StatusCode::BAD_REQUEST)?;

    if secp.verify_ecdsa(&message, &signature, &participant_pubkey.0).is_err() {
        println!("❌ Signature verification failed for episode {episode_id}");
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Update in-memory state to reflect authentication (Organizer's view)
    {
        let mut episodes = state.blockchain_episodes.lock().unwrap();
        if let Some(episode) = episodes.get_mut(&episode_id) {
            let metadata = PayloadMetadata {
                accepting_hash: 0u64.into(),
                accepting_daa: 0,
                accepting_time: 0,
                tx_id: episode_id.into(),
                tx_outputs: None,
            };
            // Execute the authentication command in memory
            let _ = episode.execute(
                &AuthCommand::SubmitResponse { signature: req.signature.clone(), nonce: req.nonce.clone() },
                Some(participant_pubkey),
                &metadata,
            );
            println!("✅ Episode {episode_id} in-memory state updated to authenticated.");
        }
    }

    Ok(Json(VerifyResponse {
        episode_id,
        authenticated: true, // Now true after in-memory verification
        status: "verification_successful_awaiting_blockchain_confirmation".to_string(),
        transaction_id: None, // Organizer does not submit transactions
    }))
}
