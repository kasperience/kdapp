// src/api/http/organizer_peer.rs
use crate::wallet::{get_wallet_for_command, get_wallet_for_command_with_storage};
use axum::serve;
use axum::{
    extract::State,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::api::http::websocket::websocket_handler;
use crate::api::http::{
    blockchain_engine::AuthHttpPeer,
    handlers::{auth::start_auth, challenge::request_challenge, revoke::revoke_session, status::get_status, verify::verify_auth},
    state::{PeerState, WebSocketMessage},
};
use axum::Json;
use kaspa_addresses::{Address, Prefix, Version};
use serde_json::json;

// Simple endpoint handlers
async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "service": "kaspa-auth-http-peer",
        "version": "0.1.0"
    }))
}

async fn funding_info(State(state): State<PeerState>) -> Json<serde_json::Value> {
    let kaspa_addr = Address::new(Prefix::Testnet, Version::PubKey, &state.peer_keypair.x_only_public_key().0.serialize());

    Json(json!({
        "funding_address": kaspa_addr.to_string(),
        "network": "testnet-10",
        "transaction_prefix": "0x41555448",
        "transaction_prefix_meaning": "AUTH"
    }))
}

async fn wallet_debug() -> Json<serde_json::Value> {
    let mut debug_info = json!({});

    // Check all wallet types
    let wallet_types = vec![("organizer-peer", "organizer-peer-wallet.key"), ("http-peer", "organizer-peer-wallet.key")];

    for (command, expected_file) in wallet_types {
        match get_wallet_for_command(command, None, ".") {
            Ok(wallet) => {
                let public_key_hex = hex::encode(wallet.keypair.public_key().serialize());
                let kaspa_addr = Address::new(Prefix::Testnet, Version::PubKey, &wallet.keypair.public_key().serialize()[1..]);

                debug_info[command] = json!({
                    "public_key": public_key_hex,
                    "kaspa_address": kaspa_addr.to_string(),
                    "expected_file": expected_file,
                    "was_created": wallet.was_created
                });
            }
            Err(e) => {
                debug_info[command] = json!({
                    "error": format!("Failed to load wallet: {}", e),
                    "expected_file": expected_file
                });
            }
        }
    }

    Json(debug_info)
}

async fn episode_authenticated(State(state): State<PeerState>, Json(payload): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let episode_id = payload["episode_id"].as_u64().unwrap_or(0);
    let challenge = payload["challenge"].as_str().unwrap_or("");

    // Get the real session token from blockchain episode
    let real_session_token = if let Ok(episodes) = state.blockchain_episodes.lock() {
        if let Some(episode) = episodes.get(&episode_id) {
            episode.session_token.clone()
        } else {
            None
        }
    } else {
        None
    };

    // Broadcast WebSocket message for authentication success
    let ws_message = WebSocketMessage {
        message_type: "authentication_successful".to_string(),
        episode_id: Some(episode_id),
        authenticated: Some(true),
        challenge: Some(challenge.to_string()),
        session_token: real_session_token,
    };

    // Send to all connected WebSocket participant peers
    let _ = state.websocket_tx.send(ws_message);

    Json(json!({
        "status": "success",
        "episode_id": episode_id,
        "message": "Authentication notification sent"
    }))
}

async fn session_revoked(State(state): State<PeerState>, Json(payload): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let episode_id = payload["episode_id"].as_u64().unwrap_or(0);
    let session_token = payload["session_token"].as_str().unwrap_or("");

    println!("🔔 Received session revocation notification for episode {}, token: {}", episode_id, session_token);

    // Broadcast WebSocket message for session revocation success
    let ws_message = WebSocketMessage {
        message_type: "session_revoked".to_string(),
        episode_id: Some(episode_id),
        authenticated: Some(false),
        challenge: None,
        session_token: Some(session_token.to_string()),
    };

    // Send to all connected WebSocket participant peers
    match state.websocket_tx.send(ws_message) {
        Ok(_) => {
            println!("✅ Session revocation WebSocket message sent for episode {}", episode_id);
        }
        Err(e) => {
            println!("❌ Failed to send session revocation WebSocket message: {}", e);
        }
    }

    Json(json!({
        "status": "success",
        "episode_id": episode_id,
        "session_token": session_token,
        "message": "Session revocation notification sent"
    }))
}

pub async fn run_http_peer(
    provided_private_key: Option<&str>,
    port: u16,
    use_keychain: bool,
    dev_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let wallet = get_wallet_for_command_with_storage("http-peer", provided_private_key, use_keychain, dev_mode, ".")?;
    let keypair = wallet.keypair;

    println!("🚀 Starting HTTP coordination peer with REAL kdapp blockchain integration");

    let (websocket_tx, _) = broadcast::channel::<WebSocketMessage>(100);

    // Create the AuthHttpPeer with kdapp engine
    let auth_peer = Arc::new(AuthHttpPeer::new(keypair, websocket_tx.clone()).await?);
    let peer_state = PeerState {
        episodes: auth_peer.peer_state.episodes.clone(),
        blockchain_episodes: auth_peer.peer_state.blockchain_episodes.clone(),
        websocket_tx: auth_peer.peer_state.websocket_tx.clone(),
        peer_keypair: auth_peer.peer_state.peer_keypair,
        transaction_generator: auth_peer.peer_state.transaction_generator.clone(),
        kaspad_client: auth_peer.peer_state.kaspad_client.clone(),
        auth_http_peer: Some(auth_peer.clone()), // Pass the Arc<AuthHttpPeer> here
    };

    let cors = CorsLayer::new().allow_origin(Any).allow_methods(tower_http::cors::AllowMethods::any()).allow_headers(Any);

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/health", get(health))
        .route("/funding-info", get(funding_info))
        .route("/wallet/debug", get(wallet_debug))
        .route("/auth/start", post(start_auth))
        .route("/auth/request-challenge", post(request_challenge))
        .route("/auth/verify", post(verify_auth))
        .route("/auth/revoke-session", post(revoke_session))
        .route("/auth/status/{episode_id}", get(get_status))
        .route("/internal/episode-authenticated", post(episode_authenticated))
        .route("/internal/session-revoked", post(session_revoked))
        .fallback_service(ServeDir::new("public"))
        .with_state(peer_state)
        .layer(cors);

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    println!("🚀 HTTP Authentication Coordination Peer starting on port {}", port);
    println!("🔗 Starting kdapp blockchain engine...");

    // Start the blockchain listener in the background
    let auth_peer_clone = auth_peer.clone();
    tokio::spawn(async move {
        if let Err(e) = auth_peer_clone.start_blockchain_listener().await {
            eprintln!("❌ Blockchain listener error: {}", e);
        }
    });

    // Start the HTTP coordination peer
    println!("🔗 kdapp engine started - HTTP coordination peer is now a real blockchain node!");
    println!("🌐 Web dashboard available at: http://localhost:{}/", port);
    serve(listener, app.into_make_service()).await?;

    Ok(())
}
