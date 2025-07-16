// src/api/http/handlers/comment.rs
use axum::{
    extract::{Json, State},
    response::Json as ResponseJson,
    http::StatusCode,
};
use log::{info, error};
use crate::api::http::{
    state::PeerState,
    types::{SubmitCommentRequest, SubmitCommentResponse, GetCommentsRequest, GetCommentsResponse},
};

pub async fn submit_comment(
    State(state): State<PeerState>,
    Json(request): Json<SubmitCommentRequest>,
) -> Result<ResponseJson<SubmitCommentResponse>, StatusCode> {
    info!("🔥 COMMENT SUBMIT: episode_id={}, text_length={}", request.episode_id, request.text.len());
    
    // Basic validation
    if request.text.trim().is_empty() {
        error!("Comment text is empty");
        return Err(StatusCode::BAD_REQUEST);
    }
    
    if request.text.len() > 1000 {
        error!("Comment text too long: {} characters", request.text.len());
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Verify session token exists and is valid
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
    
    // Verify authentication
    if !episode.is_authenticated {
        error!("Episode {} is not authenticated", request.episode_id);
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    // Verify session token matches
    if let Some(ref session_token) = episode.session_token {
        if *session_token != request.session_token {
            error!("Session token mismatch for episode {}", request.episode_id);
            return Err(StatusCode::UNAUTHORIZED);
        }
    } else {
        error!("No session token for episode {}", request.episode_id);
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    // Create comment command
    let comment_command = AuthCommand::SubmitComment {
        content: request.text.clone(),
    };

    // Get participant wallet
    let participant_wallet = match state.get_participant_wallet().await {
        Ok(wallet) => wallet,
        Err(e) => {
            error!("Failed to get participant wallet: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Build and submit transaction
    let tx_id = match state.submit_command_transaction(
        &participant_wallet,
        request.episode_id,
        comment_command,
    ).await {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to submit comment transaction: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let response = SubmitCommentResponse {
        episode_id: request.episode_id,
        comment_id: 0, // Comment ID will be determined by blockchain order
        transaction_id: Some(tx_id.to_string()),
        status: "comment_submitted".to_string(),
    };
    
    info!("✅ COMMENT SUBMITTED: episode_id={}, tx_id={}", request.episode_id, tx_id);
    Ok(ResponseJson(response))
}

pub async fn get_comments(
    State(state): State<PeerState>,
    Json(request): Json<GetCommentsRequest>,
) -> Result<ResponseJson<GetCommentsResponse>, StatusCode> {
    info!("📚 GET COMMENTS: episode_id={}", request.episode_id);
    
    // Check if episode exists
    let episode_exists = {
        let episodes = state.blockchain_episodes.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        episodes.contains_key(&request.episode_id)
    };
    
    if !episode_exists {
        error!("Episode {} not found", request.episode_id);
        return Err(StatusCode::NOT_FOUND);
    }
    
    // Retrieve comments from blockchain/episode state
    let comments = {
        let episodes = state.blockchain_episodes.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        match episodes.get(&request.episode_id) {
            Some(episode) => episode.comments.clone(),
            None => {
                error!("Episode {} not found", request.episode_id);
                return Err(StatusCode::NOT_FOUND);
            }
        }
    };
    
    let response = GetCommentsResponse {
        episode_id: request.episode_id,
        comments: comments,
        status: "comments_retrieved".to_string(),
    };
    
    info!("✅ COMMENTS RETRIEVED: episode_id={}, count={}", request.episode_id, response.comments.len());
    Ok(ResponseJson(response))
}