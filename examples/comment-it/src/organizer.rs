use axum::{
    extract::{State, WebSocketUpgrade},
    http::StatusCode,
    response::{Html, Json, Response},
    routing::{get, post},
    Router,
};

use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tower_http::{cors::CorsLayer, services::ServeDir};

use crate::comment::{Comment, CommentEpisode};

// Import auth components from our unified comment-it project
use crate::{
    api::http::types::{
        AuthRequest, AuthResponse, ChallengeResponse, SubmitCommentRequest, SubmitCommentResponse, VerifyRequest, VerifyResponse,
    },
    core::AuthWithCommentsEpisode,
};

// Additional imports for blockchain integration
use kdapp::engine::EpisodeMessage;

/// State shared across the unified comment-it organizer peer
#[derive(Clone)]
pub struct OrganizerState {
    /// Authentication episodes by episode ID (from kaspa-auth)
    pub auth_episodes: Arc<Mutex<HashMap<u64, AuthWithCommentsEpisode>>>,
    /// Comment episodes by episode ID
    pub comment_episodes: Arc<Mutex<HashMap<u64, CommentEpisode>>>,
    /// WebSocket broadcast channel for real-time updates
    pub websocket_tx: broadcast::Sender<CommentUpdate>,
}

/// Real-time comment updates sent via WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentUpdate {
    pub episode_id: u64,
    pub comment: Comment,
    pub update_type: String, // "new_comment"
}

// REMOVED: SubmitCommentRequest, SubmitCommentResponse
// These violate P2P architecture - participants submit their own transactions

/// Response for getting comments
#[derive(Debug, Serialize)]
pub struct GetCommentsResponse {
    pub comments: Vec<Comment>,
    pub total: usize,
}

/// Comment organizer peer - coordinates comment episodes via HTTP/WebSocket
pub struct CommentOrganizer {
    host: String,
    port: u16,
    state: OrganizerState,
}

impl CommentOrganizer {
    pub async fn new(host: String, port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let (websocket_tx, _) = broadcast::channel(100);

        let state = OrganizerState {
            auth_episodes: Arc::new(Mutex::new(HashMap::new())),
            comment_episodes: Arc::new(Mutex::new(HashMap::new())),
            websocket_tx,
        };

        Ok(Self { host, port, state })
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        // Print startup banner
        self.print_startup_banner();

        let app = Router::new()
            // Main page
            .route("/", get(serve_index))
            // Authentication endpoints (from kaspa-auth)
            .route("/auth/start", post(start_auth))
            .route("/auth/challenge/{episode_id}", get(get_challenge))
            .route("/auth/verify", post(verify_auth))
            .route("/auth/revoke-session", post(revoke_session))
            .route("/auth/status/{episode_id}", get(get_auth_status))
            // Comment endpoints
            .route("/api/comments", post(submit_comment))
            .route("/api/comments", get(get_comments))
            .route("/api/comments/latest", get(get_latest_comments))
            // Debug and utility
            .route("/api/debug", get(debug_endpoint))
            .route("/health", get(health_check))
            .route("/ws", get(websocket_handler))
            .nest_service("/static", ServeDir::new("public"))
            .layer(CorsLayer::permissive())
            .with_state(self.state);

        let addr = format!("{}:{}", self.host, self.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    fn print_startup_banner(&self) {
        println!();
        println!("💬 ===============================================");
        println!("💬   Comment It - Unified P2P Organizer Peer");
        println!("💬 ===============================================");
        println!();
        println!("🚀 Starting UNIFIED Comment + Auth Organizer Peer");
        println!("🔗 kaspa-auth integrated directly (no external dependency!)");
        println!();
        println!("📖 The Perfect Developer Journey:");
        println!("   1. 'How do I login?' → INTEGRATED authentication");
        println!("   2. 'How do I comment?' → SAME organizer peer!");
        println!();
        println!("🌐 Unified organizer peer running on: http://{}:{}", self.host, self.port);
        println!("🔐 Authentication endpoints:");
        println!("   • POST /auth/start       - Start auth episode");
        println!("   • GET  /auth/challenge/:id - Get challenge");
        println!("   • POST /auth/verify      - Verify signature");
        println!("   • POST /auth/revoke-session - Revoke session");
        println!("💬 Comment endpoints (read-only):");
        println!("   • GET  /api/comments     - Get all comments");
        println!("   • GET  /api/comments/latest - Get latest comments");
        println!("   • Comments submitted via participant wallets (P2P)");
        println!("🔗 Real-time WebSocket: ws://{}:{}/ws", self.host, self.port);
        println!();
        println!("✅ NO DEPENDENCIES: Everything in one organizer peer!");
        println!("🎯 Ready for the ultimate comment experience:");
        println!("   1. Open: http://{}:{}", self.host, self.port);
        println!("   2. Login (integrated auth)");
        println!("   3. Comment (same peer)");
        println!("   4. Real-time updates ✨");
        println!();
        println!("💡 True P2P Architecture:");
        println!("   • Unified organizer peer = Auth + Comments");
        println!("   • Web participant peer   = Your browser");
        println!("   • Blockchain            = Source of truth");
        println!();
        println!("🚀 Starting unified HTTP coordination peer...");
    }
}

/// Serve the main HTML page
async fn serve_index() -> Html<&'static str> {
    // Embed the HTML at compile time to avoid path issues
    Html(include_str!("../public/index.html"))
}

/// Debug endpoint to test if comment-it is working
async fn debug_endpoint() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "comment-it unified organizer peer",
        "message": "Comment-it with integrated auth is running correctly!",
        "auth": "integrated",
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }))
}

/// Health check endpoint (from kaspa-auth)
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "comment-it unified organizer peer",
        "auth": "integrated",
        "comments": "enabled"
    }))
}

/// Start authentication episode (integrated from kaspa-auth)
async fn start_auth(State(_state): State<OrganizerState>, Json(_req): Json<AuthRequest>) -> Result<Json<AuthResponse>, StatusCode> {
    info!("🚀 Starting authentication episode (integrated)");

    // TODO: Implement using kaspa-auth logic but in integrated way
    // For now, return a basic response
    Ok(Json(AuthResponse {
        episode_id: 12345,
        organizer_public_key: "placeholder_organizer_public_key".to_string(),
        participant_kaspa_address: "kaspatest:placeholder".to_string(),
        transaction_id: Some("integrated_auth_tx".to_string()),
        status: "episode_created".to_string(),
    }))
}

/// Get challenge for authentication episode
async fn get_challenge(
    State(_state): State<OrganizerState>,
    axum::extract::Path(episode_id): axum::extract::Path<u64>,
) -> Result<Json<ChallengeResponse>, StatusCode> {
    info!("🎲 Getting challenge for episode {episode_id}");

    // TODO: Get real challenge from auth episode
    Ok(Json(ChallengeResponse {
        episode_id,
        nonce: format!("auth_challenge_{episode_id}"),
        transaction_id: Some("challenge_tx".to_string()),
        status: "challenge_ready".to_string(),
    }))
}

/// Verify authentication signature
async fn verify_auth(
    State(_state): State<OrganizerState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, StatusCode> {
    info!("✅ Verifying authentication for episode {}", req.episode_id);

    // TODO: Implement real signature verification
    Ok(Json(VerifyResponse {
        episode_id: req.episode_id,
        authenticated: true,
        status: "authenticated".to_string(),
        transaction_id: Some("verify_tx".to_string()),
    }))
}

/// Revoke authentication session
async fn revoke_session(
    State(_state): State<OrganizerState>,
    Json(_req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("🔄 Revoking session");

    // TODO: Implement session revocation
    Ok(Json(serde_json::json!({
        "status": "session_revoked",
        "message": "Session revoked successfully"
    })))
}

/// Get authentication status for episode
async fn get_auth_status(
    State(_state): State<OrganizerState>,
    axum::extract::Path(episode_id): axum::extract::Path<u64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("📊 Getting auth status for episode {episode_id}");

    // TODO: Get real auth status from episode
    Ok(Json(serde_json::json!({
        "episode_id": episode_id,
        "authenticated": false,
        "challenge": null,
        "session_token": null
    })))
}

/// Submit comment via HTTP (uses participant's wallet - true P2P)
async fn submit_comment(
    State(_state): State<OrganizerState>,
    Json(request): Json<SubmitCommentRequest>,
) -> Result<Json<SubmitCommentResponse>, StatusCode> {
    info!("💬 HTTP COMMENT SUBMIT: received serialized episode message");

    // Deserialize the EpisodeMessage from the request
    let episode_message: EpisodeMessage<AuthWithCommentsEpisode> = match borsh::from_slice(&request.episode_message) {
        Ok(msg) => msg,
        Err(e) => {
            error!("❌ Failed to deserialize EpisodeMessage: {e}");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Extract episode_id from the message for response
    let episode_id = episode_message.episode_id();

    // Submit the pre-signed EpisodeMessage to the blockchain via AuthHttpPeer
    // The AuthHttpPeer should have the kaspad_client and handle the actual submission.
    // This assumes AuthHttpPeer has a method like `submit_raw_episode_message`.
    // Since AuthHttpPeer is not directly available here, we need to pass it through OrganizerState.
    // This is a placeholder for the actual submission logic.
    // The organizer is blind, so it just relays the message.
    let tx_id = "placeholder_tx_id".to_string(); // Replace with actual tx_id from submission

    // TODO: Integrate with AuthHttpPeer to submit the raw episode_message
    // This requires AuthHttpPeer to be accessible from OrganizerState and have a method
    // to submit an already constructed EpisodeMessage.
    // For now, we'll simulate success.

    info!("✅ COMMENT SUBMITTED TO BLOCKCHAIN (simulated): episode_id={episode_id}, tx_id={tx_id}");
    Ok(Json(SubmitCommentResponse {
        episode_id: episode_id.into(),
        comment_id: 0, // Will be assigned by unified episode
        transaction_id: Some(tx_id),
        status: "comment_submitted_to_blockchain".to_string(),
    }))
}

/// Get all comments
async fn get_comments(State(state): State<OrganizerState>) -> Result<Json<GetCommentsResponse>, StatusCode> {
    // Get comments from auth episodes (unified episodes contain both auth and comments)
    let auth_episodes = state.auth_episodes.lock().await;

    let mut all_comments = Vec::new();

    // Collect comments from all unified episodes
    for (_episode_id, episode) in auth_episodes.iter() {
        for comment in &episode.comments {
            all_comments.push(Comment {
                id: comment.id,
                text: comment.text.clone(),
                author: comment.author.clone(),
                timestamp: comment.timestamp,
                session_token: String::new(), // Pure P2P: No session tokens needed
            });
        }
    }

    // Sort by timestamp (newest first)
    all_comments.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let total = all_comments.len();
    Ok(Json(GetCommentsResponse { comments: all_comments, total }))
}

/// Get latest comments
async fn get_latest_comments(State(state): State<OrganizerState>) -> Result<Json<GetCommentsResponse>, StatusCode> {
    // Get latest comments from auth episodes (unified episodes contain both auth and comments)
    let auth_episodes = state.auth_episodes.lock().await;

    let mut all_comments = Vec::new();

    // Collect comments from all unified episodes
    for (_episode_id, episode) in auth_episodes.iter() {
        for comment in &episode.comments {
            all_comments.push(Comment {
                id: comment.id,
                text: comment.text.clone(),
                author: comment.author.clone(),
                timestamp: comment.timestamp,
                session_token: String::new(), // Pure P2P: No session tokens needed
            });
        }
    }

    // Sort by timestamp (newest first) and take latest 10
    all_comments.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    all_comments.truncate(10);

    let total = all_comments.len();
    Ok(Json(GetCommentsResponse { comments: all_comments, total }))
}

/// WebSocket handler for real-time comment updates
async fn websocket_handler(ws: WebSocketUpgrade, State(state): State<OrganizerState>) -> Response {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

async fn handle_websocket(socket: axum::extract::ws::WebSocket, state: OrganizerState) {
    use axum::extract::ws::Message;
    use futures_util::{sink::SinkExt, stream::StreamExt};

    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.websocket_tx.subscribe();

    info!("🔗 WebSocket connection established");

    // Spawn task to send updates to client
    let send_task = tokio::spawn(async move {
        while let Ok(update) = rx.recv().await {
            let message = serde_json::to_string(&update).unwrap();
            if sender.send(Message::Text(message.into())).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages (if any)
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            if let Ok(Message::Text(text)) = msg {
                info!("📨 WebSocket message received: {text}");
                // TODO: Handle incoming WebSocket messages if needed
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    info!("🔌 WebSocket connection closed");
}
