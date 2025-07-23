use axum::{extract::{Json, State}, response::Json as ResponseJson, http::StatusCode};
use log::{info, error};
use crate::api::http::{state::PeerState, types::{SubmitCommentRequest, SubmitCommentResponse, GetCommentsRequest, GetCommentsResponse, CommentData}};
use crate::core::AuthWithCommentsEpisode;
use kdapp::engine::EpisodeMessage;

pub async fn submit_comment(
    State(state): State<PeerState>,
    Json(request): Json<SubmitCommentRequest>,
) -> Result<ResponseJson<SubmitCommentResponse>, StatusCode> {
    info!("💬 HTTP COMMENT SUBMIT: received serialized episode message");

    // Deserialize the EpisodeMessage from the request
    let episode_message: EpisodeMessage<AuthWithCommentsEpisode> = match borsh::from_slice(&request.episode_message) {
        Ok(msg) => msg,
        Err(e) => {
            error!("❌ Failed to deserialize EpisodeMessage: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Extract episode_id from the message for response
    let episode_id = episode_message.episode_id();

    // Submit the pre-signed EpisodeMessage to the blockchain via AuthHttpPeer
    if let Some(auth_peer) = &state.auth_http_peer {
        match auth_peer.submit_episode_message_transaction(episode_message).await {
            Ok(tx_id) => {
                info!("✅ COMMENT SUBMITTED TO BLOCKCHAIN: episode_id={}, tx_id={}", episode_id, tx_id);
                Ok(ResponseJson(SubmitCommentResponse {
                    episode_id: episode_id.into(),
                    comment_id: 0, // Will be assigned by unified episode
                    transaction_id: Some(tx_id),
                    status: "comment_submitted_to_blockchain".to_string(),
                }))
            }
            Err(e) => {
                error!("❌ Comment submission failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        error!("❌ AuthHttpPeer not available");
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

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
