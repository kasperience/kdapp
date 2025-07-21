// src/api/http/handlers/comment.rs
use axum::{
    extract::{Json, State},
    response::Json as ResponseJson,
    http::StatusCode,
};
use log::{info, error};
use crate::api::http::{
    state::PeerState,
    types::{SubmitCommentRequest, SubmitCommentResponse, GetCommentsRequest, GetCommentsResponse, CommentData, SimpleCommentRequest},
};

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

// Simple comment submission endpoint (matches frontend expectations)
pub async fn submit_simple_comment(
    State(state): State<PeerState>,
    Json(request): Json<SimpleCommentRequest>,
) -> Result<ResponseJson<SubmitCommentResponse>, StatusCode> {
    info!("💬 SIMPLE COMMENT SUBMIT: episode_id={}, text_length={}", request.episode_id, request.text.len());
    
    // Get the episode to validate session token
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
    
    // Validate session token
    let episode_token = episode.session_token();
    info!("🔍 DEBUG: Episode session token: {:?}", episode_token);
    info!("🔍 DEBUG: Request session token: {:?}", request.session_token);
    
    if episode_token != Some(request.session_token.clone()) {
        error!("❌ Session token mismatch for episode {}", request.episode_id);
        error!("   Episode has: {:?}", episode_token);
        error!("   Request sent: {:?}", request.session_token);
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    if !episode.is_authenticated() {
        error!("Episode {} not authenticated", request.episode_id);
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    // Get participant wallet to create the signed command
    use crate::wallet::get_wallet_for_command;
    let participant_wallet = match get_wallet_for_command("web-participant", None) {
        Ok(wallet) => wallet,
        Err(e) => {
            error!("Failed to load participant wallet: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    
    // Create SubmitComment command
    use crate::core::UnifiedCommand;
    let submit_command = UnifiedCommand::SubmitComment {
        text: request.text,
        session_token: request.session_token,
    };
    
    // Create signed EpisodeMessage
    let pubkey = kdapp::pki::PubKey(participant_wallet.keypair.public_key());
    let episode_message = EpisodeMessage::<AuthWithCommentsEpisode>::new_signed_command(
        request.episode_id as u32,
        submit_command,
        participant_wallet.keypair.secret_key(),
        pubkey,
    );
    
    // Submit to blockchain via AuthHttpPeer
    if let Some(auth_peer) = &state.auth_http_peer {
        match auth_peer.submit_episode_message_transaction(episode_message).await {
            Ok(tx_id) => {
                info!("✅ SIMPLE COMMENT SUBMITTED TO BLOCKCHAIN: episode_id={}, tx_id={}", request.episode_id, tx_id);
                Ok(ResponseJson(SubmitCommentResponse {
                    episode_id: request.episode_id,
                    comment_id: 0, // Will be assigned by unified episode
                    transaction_id: Some(tx_id),
                    status: "comment_submitted_to_blockchain".to_string(),
                }))
            }
            Err(e) => {
                error!("❌ Simple comment submission failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        error!("❌ AuthHttpPeer not available");
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}