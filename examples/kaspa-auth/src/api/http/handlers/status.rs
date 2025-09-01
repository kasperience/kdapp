// src/api/http/handlers/status.rs
use crate::api::http::state::PeerState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::json;

pub async fn get_status(State(state): State<PeerState>, Path(episode_id): Path<u64>) -> Result<Json<serde_json::Value>, StatusCode> {
    println!("🔍 Querying episode {episode_id} from REAL blockchain state (not memory!)");

    // ✅ NEW: Query from real blockchain episodes (shared state with kdapp engine)
    match state.blockchain_episodes.lock() {
        Ok(episodes) => {
            if let Some(episode) = episodes.get(&episode_id) {
                println!("✅ Found episode {episode_id} in blockchain state");
                println!("   - Authenticated: {}", episode.is_authenticated);
                println!("   - Challenge: {:?}", episode.challenge);
                println!("   - Session token: {:?}", episode.session_token);

                Ok(Json(json!({
                    "episode_id": episode_id,
                    "authenticated": episode.is_authenticated,
                    "status": if episode.is_authenticated { "authenticated" } else { "pending" },
                    "challenge": episode.challenge,
                    "session_token": episode.session_token,
                    "blockchain_confirmed": true,
                    "public_key": episode.owner.map(|pk| hex::encode(pk.0.serialize())),
                    "source": "real_blockchain_state"
                })))
            } else {
                println!("⚠️ Episode {episode_id} not found in blockchain state");

                Ok(Json(json!({
                    "episode_id": episode_id,
                    "authenticated": false,
                    "status": "episode_not_found",
                    "challenge": null,
                    "session_token": null,
                    "blockchain_confirmed": false,
                    "message": "Episode not found in blockchain state - may not be confirmed yet"
                })))
            }
        }
        Err(e) => {
            println!("❌ Failed to lock blockchain episodes: {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
