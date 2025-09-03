// src/api/http/handlers/auth.rs
use axum::{extract::State, http::StatusCode, response::Json};
use kaspa_addresses::{Address, Prefix, Version};

use kdapp::{engine::EpisodeMessage, pki::PubKey};
use rand::Rng;

use crate::api::http::{
    state::PeerState,
    types::{AuthRequest, AuthResponse},
};
use crate::core::AuthWithCommentsEpisode;

pub async fn start_auth(State(state): State<PeerState>, Json(req): Json<AuthRequest>) -> Result<Json<AuthResponse>, StatusCode> {
    println!("🚀 Submitting REAL NewEpisode transaction to Kaspa blockchain...");

    // Parse the participant's public key
    println!("📋 Received public key: {}", &req.public_key);
    let participant_pubkey = match hex::decode(&req.public_key) {
        Ok(bytes) => {
            println!("✅ Hex decode successful, {} bytes", bytes.len());
            match secp256k1::PublicKey::from_slice(&bytes) {
                Ok(pk) => {
                    println!("✅ Public key parsing successful");
                    PubKey(pk)
                }
                Err(e) => {
                    println!("❌ Public key parsing failed: {e}");
                    return Err(StatusCode::BAD_REQUEST);
                }
            }
        }
        Err(e) => {
            println!("❌ Hex decode failed: {e}");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Determine if we're creating a new episode or joining existing one
    let (episode_id, is_joining_existing) = match req.episode_id {
        Some(existing_id) => {
            println!("🎯 Joining existing episode: {existing_id}");
            (existing_id, true)
        }
        None => {
            let new_id = rand::thread_rng().gen();
            println!("🆕 Creating new episode: {new_id}");
            (new_id, false)
        }
    };

    // Create participant Kaspa address for transaction funding (like CLI does)
    let participant_addr = Address::new(Prefix::Testnet, Version::PubKey, &participant_pubkey.0.x_only_public_key().0.serialize());

    // 🎯 TRUE P2P: Get participant's wallet to fund their own episode creation
    let participant_wallet =
        crate::wallet::get_wallet_for_command("web-participant", None).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create participant's Kaspa address for transaction funding (True P2P!)
    let participant_funding_addr =
        Address::new(Prefix::Testnet, Version::PubKey, &participant_wallet.keypair.x_only_public_key().0.serialize());

    // Create NewEpisode message for blockchain (always, kdapp handles idempotency)
    let episode_message = Some(EpisodeMessage::<AuthWithCommentsEpisode>::NewEpisode {
        episode_id: episode_id as u32,
        participants: vec![participant_pubkey],
    });

    // Quick UTXO check (detailed UTXO handling happens in blockchain engine)
    if let Some(ref kaspad) = state.kaspad_client {
        println!("🔍 Quick check for participant wallet funding...");
        let entries = state.utxo_cache.get(kaspad, &participant_funding_addr).await.map_err(|e| {
            println!("❌ Failed to fetch UTXOs: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if entries.is_empty() {
            println!("❌ No UTXOs found! Participant wallet needs funding.");
            println!("💰 Fund this address: {participant_funding_addr}");
            println!("🚰 Get testnet funds: https://faucet.kaspanet.io/");
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }

        println!("✅ Participant wallet has UTXOs - ready for transaction");
    } else {
        println!("❌ No kaspad client available");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    println!("🎯 Episode ID: {episode_id}");
    println!("👤 Participant PubKey: {participant_pubkey}");

    let (transaction_id, status) = match episode_message {
        Some(new_episode) => {
            // ✅ Submit new episode transaction to blockchain via AuthHttpPeer
            println!("📤 Submitting transaction to Kaspa blockchain via AuthHttpPeer...");
            match state.auth_http_peer.as_ref().unwrap().submit_episode_message_transaction(new_episode).await {
                Ok(tx_id) => {
                    println!("✅ MATRIX UI SUCCESS: Auth episode created - Transaction {tx_id}");
                    println!("🎬 Episode {episode_id} initialized on blockchain");
                    (
                        tx_id,
                        if is_joining_existing {
                            "joined_existing_episode".to_string()
                        } else {
                            "submitted_to_blockchain".to_string()
                        },
                    )
                }
                Err(e) => {
                    println!("❌ MATRIX UI ERROR: Auth episode creation failed - {e}");
                    println!("💡 Make sure participant wallet is funded: {participant_funding_addr}");
                    ("error".to_string(), "transaction_submission_failed".to_string())
                }
            }
        }
        None => {
            // This branch should ideally not be reached if episode_message is always Some
            // However, for compilation, we provide a default return.
            // In a real scenario, this would be an error or a different flow.
            ("no_transaction_needed".to_string(), "joined_existing_episode".to_string())
        }
    };

    Ok(Json(AuthResponse {
        episode_id,
        organizer_public_key: hex::encode(state.peer_keypair.x_only_public_key().0.serialize()),
        participant_kaspa_address: participant_addr.to_string(),
        transaction_id: Some(transaction_id),
        status,
    }))
}
