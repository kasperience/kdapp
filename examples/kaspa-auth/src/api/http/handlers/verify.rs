// src/api/http/handlers/verify.rs - FIXED VERSION
use axum::{extract::State, response::Json, http::StatusCode};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_wrpc_client::prelude::RpcApi;
use kdapp::{
    engine::EpisodeMessage,
    pki::PubKey,
    episode::{Episode, PayloadMetadata},
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
    let participant_wallet = crate::wallet::get_wallet_for_command("web-participant", None, ".")
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
            let metadata = PayloadMetadata { 
                accepting_hash: 0u64.into(), 
                accepting_daa: 0, 
                accepting_time: 0, 
                tx_id: episode_id.into()
            };
            let _ = episode.execute(
                &AuthCommand::SubmitResponse {
                    signature: req.signature.clone(),
                    nonce: req.nonce.clone(),
                },
                Some(participant_pubkey),
                &metadata
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