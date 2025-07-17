// src/api/http/handlers/comment.rs
use axum::{
    extract::{Json, State},
    response::Json as ResponseJson,
    http::StatusCode,
};
use log::{info, error};
use crate::api::http::{
    state::PeerState,
    types::{GetCommentsRequest, GetCommentsResponse, CommentData},
};
use crate::core::AuthWithCommentsEpisode;

// REMOVED: submit_comment endpoint
// According to OVERTHINKING.md roadmap, HTTP peer should NEVER create transactions
// Participants must fund and submit their own comment transactions
// Frontend will use userWallet.createCommentTransaction() instead

pub async fn get_comments(
    State(state): State<PeerState>,
    Json(request): Json<GetCommentsRequest>,
) -> Result<ResponseJson<GetCommentsResponse>, StatusCode> {
    info!("📚 GET COMMENTS: episode_id={}", request.episode_id);
    
    // Get the unified episode from blockchain state
    let episode_state = {
        let episodes = state.blockchain_episodes.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        episodes.get(&request.episode_id).cloned()
    };
    
    let episode = match episode_state {
        Some(episode) => episode,
        None => {
            error!("Episode {} not found", request.episode_id);
            return Err(StatusCode::NOT_FOUND);
        }
    };
    
    // Convert episode comments to API format
    let comments: Vec<CommentData> = episode.comments.iter().map(|comment| {
        CommentData {
            id: comment.id,
            text: comment.text.clone(),
            author: format!("{}", comment.author),
            timestamp: comment.timestamp,
        }
    }).collect();
    
    let response = GetCommentsResponse {
        episode_id: request.episode_id,
        comments,
        status: "comments_retrieved".to_string(),
    };
    
    info!("✅ COMMENTS RETRIEVED: episode_id={}, count={}", request.episode_id, response.comments.len());
    Ok(ResponseJson(response))
}