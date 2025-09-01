// src/api/http/handlers/challenge.rs - FIXED VERSION
use crate::api::http::{
    state::PeerState,
    types::{ChallengeRequest, ChallengeResponse},
};
use crate::core::commands::AuthCommand;
use axum::{extract::State, http::StatusCode, response::Json};
use kdapp::episode::{Episode, PayloadMetadata};

/// Request challenge - HTTP coordination only, no blockchain!
pub async fn request_challenge(
    State(state): State<PeerState>,
    Json(req): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, StatusCode> {
    println!("📨 Processing challenge request (HTTP coordination)...");

    let episode_id: u64 = req.episode_id;

    // Execute challenge generation in memory
    {
        let mut episodes = state.blockchain_episodes.lock().unwrap();
        if let Some(episode) = episodes.get_mut(&episode_id) {
            // Generate challenge locally for coordination
            let challenge_cmd = AuthCommand::RequestChallenge;
            let metadata = PayloadMetadata {
                accepting_hash: 0u64.into(),
                accepting_daa: 0,
                accepting_time: 0,
                tx_id: episode_id.into(),
                tx_outputs: None,
            };
            match episode.execute(&challenge_cmd, episode.owner, &metadata) {
                Ok(_) => {
                    if let Some(challenge) = &episode.challenge {
                        println!("✅ Challenge generated: {challenge}");

                        // Return challenge immediately (no blockchain wait!)
                        return Ok(Json(ChallengeResponse {
                            episode_id,
                            nonce: challenge.clone(),
                            transaction_id: None, // No transaction!
                            status: "challenge_ready".to_string(),
                        }));
                    }
                }
                Err(e) => {
                    println!("❌ Challenge generation failed: {e:?}");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }

    Err(StatusCode::NOT_FOUND)
}
