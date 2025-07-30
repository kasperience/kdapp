// src/api/http/handlers/auth.rs - FIXED VERSION
use axum::{extract::State, response::Json, http::StatusCode};
use kaspa_addresses::{Address, Prefix, Version};
use kdapp::pki::PubKey;
use rand::Rng;
use crate::api::http::{
    types::{AuthRequest, AuthResponse},
    state::PeerState,
};
use crate::core::episode::SimpleAuth;

/// Start authentication - HTTP coordination only, no blockchain!
pub async fn start_auth(
    State(state): State<PeerState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    println!("🚀 Starting authentication episode (HTTP coordination)...");
    
    // Parse participant's public key
    let participant_pubkey = match hex::decode(&req.public_key) {
        Ok(bytes) => match secp256k1::PublicKey::from_slice(&bytes) {
            Ok(pk) => PubKey(pk),
            Err(_) => return Err(StatusCode::BAD_REQUEST),
        },
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };
    
    // Generate episode ID
    let episode_id: u64 = rand::thread_rng().gen();
    
    // Create participant address for display
    let participant_addr = Address::new(
        Prefix::Testnet, 
        Version::PubKey, 
        &participant_pubkey.0.x_only_public_key().0.serialize()
    );
    
    // Create in-memory episode for coordination
    let mut episode = SimpleAuth::initialize(
        vec![participant_pubkey],
        &kdapp::episode::PayloadMetadata::default()
    );
    
    // Store in coordination state (NOT blockchain yet!)
    {
        let mut episodes = state.blockchain_episodes.lock().unwrap();
        episodes.insert(episode_id, episode);
    }
    
    println!("✅ Episode {} created for HTTP coordination", episode_id);
    println!("📝 Participant should submit NewEpisode transaction themselves");
    
    Ok(Json(AuthResponse {
        episode_id,
        organizer_public_key: hex::encode(state.peer_keypair.public_key().serialize()),
        participant_kaspa_address: participant_addr.to_string(),
        transaction_id: None, // No transaction - just coordination!
        status: "episode_created_awaiting_blockchain".to_string(),
    }))
}

// src/api/http/handlers/challenge.rs - FIXED VERSION
use axum::{extract::State, response::Json, http::StatusCode};
use crate::api::http::{
    types::{ChallengeRequest, ChallengeResponse},
    state::PeerState,
};
use crate::core::commands::AuthCommand;

/// Request challenge - HTTP coordination only, no blockchain!
pub async fn request_challenge(
    State(state): State<PeerState>,
    Json(req): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, StatusCode> {
    println!("📨 Processing challenge request (HTTP coordination)...");
    
    let episode_id: u64 = req.episode_id;
    
    // Execute challenge generation in memory
    {
        let mut episodes = state.blockchain_episodes.lock().unwrap();
        if let Some(episode) = episodes.get_mut(&episode_id) {
            // Generate challenge locally for coordination
            let challenge_cmd = AuthCommand::RequestChallenge;
            match episode.execute(
                &challenge_cmd,
                episode.owner,
                &kdapp::episode::PayloadMetadata::default()
            ) {
                Ok(_) => {
                    if let Some(challenge) = &episode.challenge {
                        println!("✅ Challenge generated: {}", challenge);
                        
                        // Return challenge immediately (no blockchain wait!)
                        return Ok(Json(ChallengeResponse {
                            episode_id,
                            nonce: challenge.clone(),
                            transaction_id: None, // No transaction!
                            status: "challenge_ready".to_string(),
                        }));
                    }
                }
                Err(e) => {
                    println!("❌ Challenge generation failed: {:?}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }
    
    Err(StatusCode::NOT_FOUND)
}

// src/api/http/handlers/verify.rs - FIXED VERSION
use axum::{extract::State, response::Json, http::StatusCode};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_wrpc_client::prelude::RpcApi;
use kdapp::{
    engine::EpisodeMessage,
    pki::PubKey,
};
use crate::api::http::{
    types::{VerifyRequest, VerifyResponse},
    state::PeerState,
};
use crate::core::{episode::SimpleAuth, commands::AuthCommand};

/// Verify authentication - This is where ALL blockchain transactions happen!
pub async fn verify_auth(
    State(state): State<PeerState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, StatusCode> {
    println!("🔍 Starting FULL blockchain verification flow...");
    
    let episode_id: u64 = req.episode_id;
    
    // Get episode and participant info
    let (participant_pubkey, current_challenge) = {
        let episodes = state.blockchain_episodes.lock().unwrap();
        match episodes.get(&episode_id) {
            Some(ep) => {
                if ep.is_authenticated {
                    return Ok(Json(VerifyResponse {
                        episode_id,
                        authenticated: true,
                        status: "already_authenticated".to_string(),
                        transaction_id: None,
                    }));
                }
                (ep.owner.unwrap(), ep.challenge.clone())
            }
            None => return Err(StatusCode::NOT_FOUND),
        }
    };
    
    // Verify challenge matches
    if current_challenge.as_ref() != Some(&req.nonce) {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Get participant wallet
    let participant_wallet = crate::wallet::get_wallet_for_command("web-participant", None)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let participant_addr = Address::new(
        Prefix::Testnet, 
        Version::PubKey, 
        &participant_wallet.keypair.x_only_public_key().0.serialize()
    );
    
    // Get UTXOs
    let kaspad = state.kaspad_client.as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    
    let entries = kaspad.get_utxos_by_addresses(vec![participant_addr.clone()]).await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    
    if entries.is_empty() {
        println!("❌ No UTXOs! Participant needs to fund: {}", participant_addr);
        return Err(StatusCode::PAYMENT_REQUIRED);
    }
    
    let mut utxo = (
        TransactionOutpoint::from(entries[0].outpoint.clone()),
        UtxoEntry::from(entries[0].utxo_entry.clone())
    );
    
    println!("📤 Submitting ALL 3 transactions to blockchain...");
    
    // Transaction 1: NewEpisode
    let new_episode = EpisodeMessage::<SimpleAuth>::NewEpisode { 
        episode_id: episode_id as u32, 
        participants: vec![participant_pubkey] 
    };
    
    let tx1 = state.transaction_generator.build_command_transaction(
        utxo.clone(), &participant_addr, &new_episode, 5000
    );
    
    kaspad.submit_transaction(tx1.as_ref().into(), false).await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    
    println!("✅ Transaction 1: NewEpisode submitted");
    utxo = kdapp::generator::get_first_output_utxo(&tx1);
    
    // Wait for confirmation
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    
    // Transaction 2: RequestChallenge
    let request_challenge = EpisodeMessage::<SimpleAuth>::new_signed_command(
        episode_id as u32,
        AuthCommand::RequestChallenge,
        participant_wallet.keypair.secret_key(),
        participant_pubkey
    );
    
    let tx2 = state.transaction_generator.build_command_transaction(
        utxo.clone(), &participant_addr, &request_challenge, 5000
    );
    
    kaspad.submit_transaction(tx2.as_ref().into(), false).await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    
    println!("✅ Transaction 2: RequestChallenge submitted");
    utxo = kdapp::generator::get_first_output_utxo(&tx2);
    
    // Wait for confirmation
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    
    // Transaction 3: SubmitResponse
    let submit_response = EpisodeMessage::<SimpleAuth>::new_signed_command(
        episode_id as u32,
        AuthCommand::SubmitResponse {
            signature: req.signature.clone(),
            nonce: req.nonce.clone(),
        },
        participant_wallet.keypair.secret_key(),
        participant_pubkey
    );
    
    let tx3 = state.transaction_generator.build_command_transaction(
        utxo, &participant_addr, &submit_response, 5000
    );
    
    let tx_id = tx3.id().to_string();
    
    kaspad.submit_transaction(tx3.as_ref().into(), false).await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    
    println!("✅ Transaction 3: SubmitResponse submitted");
    println!("🎯 All 3 transactions submitted successfully!");
    
    // Update in-memory state to reflect authentication
    {
        let mut episodes = state.blockchain_episodes.lock().unwrap();
        if let Some(episode) = episodes.get_mut(&episode_id) {
            // Execute the authentication in memory
            let _ = episode.execute(
                &AuthCommand::SubmitResponse {
                    signature: req.signature.clone(),
                    nonce: req.nonce.clone(),
                },
                Some(participant_pubkey),
                &kdapp::episode::PayloadMetadata::default()
            );
        }
    }
    
    Ok(Json(VerifyResponse {
        episode_id,
        authenticated: false, // Will be true once blockchain confirms
        status: "all_transactions_submitted".to_string(),
        transaction_id: Some(tx_id),
    }))
}