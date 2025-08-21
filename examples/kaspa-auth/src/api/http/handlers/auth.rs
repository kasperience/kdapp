// src/api/http/handlers/auth.rs - FIXED VERSION
use crate::api::http::{
    state::PeerState,
    types::{AuthRequest, AuthResponse},
};
use crate::core::episode::SimpleAuth;
use axum::{extract::State, http::StatusCode, response::Json};
use kaspa_addresses::{Address, Prefix, Version};
use kdapp::{
    episode::{Episode, PayloadMetadata},
    pki::PubKey,
};
use rand::Rng;

/// Start authentication - HTTP coordination only, no blockchain!
pub async fn start_auth(
    State(state): State<PeerState>,
    Json(_req): Json<AuthRequest>, // _req because public_key is not used here
) -> Result<Json<AuthResponse>, StatusCode> {
    println!("🚀 Start auth request received (HTTP coordination)...");
    println!("📝 Organizer peer is ready to coordinate. Participant should submit NewEpisode transaction.");

    Ok(Json(AuthResponse {
        organizer_public_key: hex::encode(state.peer_keypair.public_key().serialize()),
        message: "Organizer peer ready for NewEpisode transaction from participant.".to_string(),
    }))
}
