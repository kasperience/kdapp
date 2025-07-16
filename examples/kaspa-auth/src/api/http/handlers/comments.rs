// src/api/http/handlers/comments.rs
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use log::info;
use std::sync::Arc;

use crate::api::http::state::PeerState;
use crate::api::http::types::{SubmitCommentRequest, SubmitCommentResponse, CommentsResponse};
use crate::api::http::blockchain::TxSubmitter;

/// Submit a comment to an episode (P2P kdapp approach)
/// This endpoint validates the request and helps coordinate comment submission
/// The actual transaction is submitted by the participant's wallet, not the organizer
pub async fn submit_comment(
    State(state): State<PeerState>,
    Json(request): Json<SubmitCommentRequest>,
) -> Result<Json<SubmitCommentResponse>, StatusCode> {
    info!("💬 Comment submission coordination request for episode {}", request.episode_id);
    
    // Get the episode state
    let episode_state = {
        let episodes = state.blockchain_episodes.lock().unwrap();
        episodes.get(&request.episode_id).cloned()
    };
    
    let Some(episode) = episode_state else {
        info!("❌ Episode {} not found", request.episode_id);
        return Err(StatusCode::NOT_FOUND);
    };
    
    // Verify the session token matches
    if episode.session_token != Some(request.session_token.clone()) {
        info!("❌ Invalid session token for episode {}", request.episode_id);
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    // Verify the user is authenticated
    if !episode.is_authenticated {
        info!("❌ User not authenticated for episode {}", request.episode_id);
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    // kdapp P2P Philosophy: Return coordination info for participant to submit their own transaction
    // The participant will use their own wallet to submit the SubmitComment transaction
    info!("✅ Comment request validated - participant should submit their own transaction");
    
    // Return coordination response - the participant will handle the actual blockchain submission
    Ok(Json(SubmitCommentResponse {
        episode_id: request.episode_id,
        comment_id: 0, // Will be assigned by episode when transaction is processed
        transaction_id: "participant_will_submit".to_string(),
        status: "ready_for_participant_submission".to_string(),
    }))
}

/// Get all comments for an episode
pub async fn get_comments(
    State(state): State<PeerState>,
    Path(episode_id): Path<u64>,
) -> Result<Json<CommentsResponse>, StatusCode> {
    info!("📜 Get comments request for episode {}", episode_id);
    
    // Get the episode state
    let episode_state = {
        let episodes = state.blockchain_episodes.lock().unwrap();
        episodes.get(&episode_id).cloned()
    };
    
    let Some(episode) = episode_state else {
        info!("❌ Episode {} not found", episode_id);
        return Err(StatusCode::NOT_FOUND);
    };
    
    // Return all comments
    Ok(Json(CommentsResponse {
        episode_id,
        comments: episode.comments.clone(),
        total_count: episode.comments.len(),
    }))
}