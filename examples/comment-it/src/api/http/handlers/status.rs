// src/api/http/handlers/status.rs
use crate::api::http::state::PeerState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
pub struct StatusQuery {
    pub pubkey: Option<String>,
}

// #[axum::debug_handler]
pub async fn get_status(
    State(state): State<PeerState>,
    Path(episode_id): Path<u64>,
    Query(q): Query<StatusQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    println!("🎭 MATRIX UI ACTION: User checking authentication status");
    println!("🔍 Querying episode {episode_id} from REAL blockchain state (not memory!)");

    // Snapshot required fields without holding the mutex across awaits
    let snapshot = match state.blockchain_episodes.lock() {
        Ok(episodes) => {
            if let Some(episode) = episodes.get(&episode_id) {
                println!("✅ MATRIX UI SUCCESS: Found episode {episode_id} in blockchain state");
                println!("   - Authenticated: {}", episode.is_authenticated());
                println!("   - Challenge: {:?}", episode.challenge());
                println!("   - Session token: {:?}", episode.session_token());
                Some((
                    true,
                    episode.is_authenticated(),
                    episode.challenge(),
                    episode.session_token(),
                    episode.owner().map(|pk| hex::encode(pk.0.serialize())),
                ))
            } else {
                println!("⚠️ MATRIX UI ERROR: Episode {episode_id} not found in blockchain state");
                Some((false, false, None, None, None))
            }
        }
        Err(e) => {
            println!("❌ MATRIX UI ERROR: Failed to lock blockchain episodes - {e}");
            None
        }
    };

    let (found, is_authenticated, challenge, session_token, owner_hex) = match snapshot {
        Some(t) => t,
        None => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // If not authenticated or episode not found, optionally consult kdapp-indexer membership
    if !found || !is_authenticated {
        if let Some(pubkey) = q.pubkey.as_deref() {
            if let Some(via_indexer) = try_indexer_membership(episode_id, pubkey).await {
                return Ok(Json(via_indexer));
            }
        }
    }

    if found {
        // Compute deterministic session handle when a pubkey is supplied
        let session_token = if let Some(pubkey_hex) = q.pubkey.as_deref() {
            Some(deterministic_handle(episode_id, pubkey_hex))
        } else {
            session_token
        };
        Ok(Json(json!({
            "episode_id": episode_id,
            "authenticated": is_authenticated,
            "status": if is_authenticated { "authenticated" } else { "pending" },
            "challenge": challenge,
            "session_token": session_token,
            "blockchain_confirmed": true,
            "public_key": owner_hex,
            "source": "real_blockchain_state"
        })))
    } else {
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

// Attempt to consult kdapp-indexer for episode membership and surface as authenticated
async fn try_indexer_membership(episode_id: u64, pubkey: &str) -> Option<serde_json::Value> {
    let base = std::env::var("INDEXER_URL").unwrap_or_else(|_| "http://127.0.0.1:8090".to_string());
    let url = format!("{}/index/me/{}?pubkey={}", base.trim_end_matches('/'), episode_id, pubkey);
    let client = match reqwest::Client::builder().build() {
        Ok(c) => c,
        Err(_) => return None,
    };
    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(_) => return None,
    };
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(_) => return None,
    };
    if json.get("member").and_then(|m| m.as_bool()) == Some(true) {
        Some(json!({
            "episode_id": episode_id,
            "authenticated": true,
            "status": "authenticated_via_indexer",
            "challenge": null,
            "session_token": deterministic_handle(episode_id, pubkey),
            "blockchain_confirmed": false,
            "public_key": pubkey,
            "source": "indexer_membership"
        }))
    } else {
        None
    }
}

fn deterministic_handle(episode_id: u64, pubkey_hex: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"KDAPP/COMMENT-IT/SESSION");
    hasher.update(episode_id.to_be_bytes());
    // Use pubkey hex bytes for stability
    hasher.update(pubkey_hex.as_bytes());
    hex::encode(hasher.finalize())
}
