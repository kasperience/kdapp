use std::error::Error;
use secp256k1::Keypair;
use crate::core::{commands::AuthCommand, episode::SimpleAuth};
use hex;
use reqwest;
use serde_json;
use kaspa_hashes::Hash;

#[derive(Debug, Clone)]
pub struct AuthenticationResult {
    pub episode_id: u64,
    pub session_token: String,
    pub authenticated: bool,
}

/// 🚀 Working endpoint authentication - uses WORKING web UI pattern
/// This function follows the exact same pattern as the working web UI endpoints
pub async fn run_working_endpoint_authentication(kaspa_signer: Keypair, auth_signer: Keypair, peer_url: String) -> Result<AuthenticationResult, Box<dyn Error>> {
    let participant_peer = reqwest::Client::new();
    let public_key_hex = hex::encode(auth_signer.public_key().serialize());
    
    println!("🔑 Using public key: {}", public_key_hex);
    
    // Step 1: Create episode using WORKING /auth/start endpoint
    println!("🚀 Step 1: Creating episode via /auth/start...");
    let start_response = participant_peer
        .post(&format!("{}/auth/start", peer_url))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "public_key": public_key_hex
        }))
        .send()
        .await?;
    
    if !start_response.status().is_success() {
        return Err(format!("Failed to start auth: HTTP {}", start_response.status()).into());
    }
    
    let start_data: serde_json::Value = start_response.json().await?;
    let episode_id = start_data["episode_id"].as_u64()
        .ok_or("Organizer peer did not return valid episode_id")?;
    
    println!("✅ Episode {} created by organizer peer", episode_id);
    
    // Step 2: Request challenge using WORKING /auth/request-challenge endpoint
    println!("📨 Step 2: Requesting challenge via /auth/request-challenge...");
    let challenge_response = participant_peer
        .post(&format!("{}/auth/request-challenge", peer_url))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "episode_id": episode_id,
            "public_key": public_key_hex
        }))
        .send()
        .await?;
    
    if !challenge_response.status().is_success() {
        return Err(format!("Failed to request challenge: HTTP {}", challenge_response.status()).into());
    }
    
    println!("✅ Challenge request submitted");
    
    // Step 3: Poll for challenge using WORKING /auth/status/{id} endpoint
    println!("⏳ Step 3: Polling for challenge via /auth/status/{}...", episode_id);
    let mut challenge = String::new();
    
    for attempt in 1..=10 {
        println!("🔄 Polling attempt {} of 10...", attempt);
        
        let status_response = participant_peer
            .get(&format!("{}/auth/status/{}", peer_url, episode_id))
            .send()
            .await?;
        
        if status_response.status().is_success() {
            let status_data: serde_json::Value = status_response.json().await?;
            if let Some(organizer_challenge) = status_data["challenge"].as_str() {
                challenge = organizer_challenge.to_string();
                println!("🎯 Challenge retrieved from organizer peer: {}", challenge);
                break;
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }
    
    if challenge.is_empty() {
        return Err("❌ Could not retrieve challenge from organizer peer".into());
    }
    
    // Step 4: Sign challenge using organizer-side signing (like web UI)
    println!("✍️ Step 4: Signing challenge via /auth/sign-challenge...");
    let private_key_hex = hex::encode(auth_signer.secret_key().as_ref());
    let sign_response = participant_peer
        .post(&format!("{}/auth/sign-challenge", peer_url))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "challenge": challenge,
            "private_key": private_key_hex
        }))
        .send()
        .await?;
    
    if !sign_response.status().is_success() {
        return Err(format!("Failed to sign challenge: HTTP {}", sign_response.status()).into());
    }
    
    let sign_data: serde_json::Value = sign_response.json().await?;
    let signature = sign_data["signature"].as_str()
        .ok_or("Organizer peer did not return signature")?;
    
    println!("✅ Challenge signed successfully");
    
    // Step 5: Submit verification using WORKING /auth/verify endpoint
    println!("📤 Step 5: Submitting verification via /auth/verify...");
    let verify_response = participant_peer
        .post(&format!("{}/auth/verify", peer_url))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "episode_id": episode_id,
            "signature": signature,
            "nonce": challenge
        }))
        .send()
        .await?;
    
    if !verify_response.status().is_success() {
        return Err(format!("Failed to verify: HTTP {}", verify_response.status()).into());
    }
    
    println!("✅ Verification submitted");
    
    // Step 6: Wait for authentication completion using WORKING polling
    println!("⏳ Step 6: Waiting for authentication completion...");
    let mut session_token = String::new();
    
    for attempt in 1..=50 {
        let status_response = participant_peer
            .get(&format!("{}/auth/status/{}", peer_url, episode_id))
            .send()
            .await?;
        
        if status_response.status().is_success() {
            let status_data: serde_json::Value = status_response.json().await?;
            if let (Some(authenticated), Some(token)) = (
                status_data["authenticated"].as_bool(),
                status_data["session_token"].as_str()
            ) {
                if authenticated && !token.is_empty() {
                    session_token = token.to_string();
                    println!("✅ REAL session token retrieved: {}", session_token);
                    break;
                }
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    if session_token.is_empty() {
        return Err("❌ Could not retrieve session token from organizer peer".into());
    }
    
    Ok(AuthenticationResult {
        episode_id,
        session_token,
        authenticated: true,
    })
}

/// 🚀 HTTP Coordinated authentication - hybrid kdapp + HTTP coordination  
/// This function attempts to use pure kdapp authentication first, and falls back to HTTP coordination
/// for challenge retrieval if the blockchain-based challenge retrieval times out.
pub async fn run_http_coordinated_authentication(kaspa_signer: Keypair, auth_signer: Keypair, peer_url: String) -> Result<AuthenticationResult, Box<dyn Error>> {
    use kdapp::{
        engine::EpisodeMessage,
        generator::{self, TransactionGenerator},
        proxy::connect_client,
    };
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_consensus_core::{network::NetworkId, tx::{TransactionOutpoint, UtxoEntry}};
        use kaspa_wrpc_client::prelude::*;
    use kaspa_rpc_core::api::rpc::RpcApi;
    use crate::episode_runner::{AUTH_PATTERN, AUTH_PREFIX};
    use rand::Rng;
    
    let participant_pubkey = kdapp::pki::PubKey(auth_signer.public_key());
    println!("🔑 Auth public key: {}", participant_pubkey);
    
    // Connect to Kaspa network (real blockchain!)
    let network = NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 10);
    println!("📡 Connecting to testnet-10 blockchain...");
    
    let kaspad = connect_client(network, None).await?;
    
    // Create Kaspa address for funding transactions
    let kaspa_addr = Address::new(Prefix::Testnet, Version::PubKey, &kaspa_signer.x_only_public_key().0.serialize());
    println!("💰 Kaspa address: {}", kaspa_addr);
    
    // Get UTXOs for transaction funding
    println!("🔍 Fetching UTXOs...");
    let entries = kaspad.get_utxos_by_addresses(vec![kaspa_addr.clone()]).await?;
    
    if entries.is_empty() {
        return Err("No UTXOs found! Please fund the Kaspa address first.".into());
    }
    
    let mut utxo = entries.first().map(|entry| {
        (TransactionOutpoint::from(entry.outpoint.clone()), UtxoEntry::from(entry.utxo_entry.clone()))
    }).unwrap();
    
    println!("✅ UTXO found: {}", utxo.0);
    
    // Create real transaction generator (kdapp architecture!)
    let generator = TransactionGenerator::new(kaspa_signer, AUTH_PATTERN, AUTH_PREFIX);
    
    // Step 1: Request organizer peer to create and manage the authentication episode
    // The organizer peer creates episodes so its kdapp engine knows about them
    println!("🔗 Requesting organizer peer to create authentication episode...");
    
    let participant_peer = reqwest::Client::new();
    let public_key_hex = hex::encode(participant_pubkey.0.serialize());
    
    // Use the /auth/start endpoint which creates episodes on the organizer peer side
    let start_url = format!("{}/auth/start", peer_url);
    let start_response = participant_peer
        .post(&start_url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "public_key": public_key_hex
        }))
        .send()
        .await?;
    
    let start_data: serde_json::Value = start_response.json().await?;
    let episode_id = start_data["episode_id"].as_u64()
        .ok_or("Organizer peer did not return valid episode_id")?;
    
    println!("✅ Authentication episode {} created by organizer peer", episode_id);
    
    // Step 2: Send RequestChallenge command to blockchain
    println!("📨 Sending RequestChallenge command to blockchain...");
    
    let auth_command = AuthCommand::RequestChallenge;
    let step = EpisodeMessage::<SimpleAuth>::new_signed_command(
        episode_id as u32, 
        auth_command, 
        auth_signer.secret_key(), 
        participant_pubkey
    );
    
    let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, 5000);
    println!("🚀 Submitting RequestChallenge transaction: {}", tx.id());
    
    let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
    utxo = generator::get_first_output_utxo(&tx);
    
    println!("✅ RequestChallenge transaction submitted to blockchain!");
    println!("⏳ Waiting for challenge response from auth organizer peer...");
    
    // Wait for organizer peer to process RequestChallenge and generate challenge
    println!("⏳ Waiting for organizer peer to generate challenge...");
    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
    
    let mut challenge = String::new();
    let participant_peer = reqwest::Client::new();
    
    // Get challenge via HTTP (polling until available)
    for retry_attempt in 1..=10 {
        println!("🔄 Checking for challenge attempt {} of 10...", retry_attempt);
        
        let status_url = format!("{}/auth/status/{}", peer_url, episode_id);
        
        match participant_peer.get(&status_url).send().await {
            Ok(response) if response.status().is_success() => {
                if let Ok(status_json) = response.text().await {
                    println!("📡 HTTP status response: {}", status_json);
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&status_json) {
                        if let Some(organizer_challenge) = parsed["challenge"].as_str() {
                            challenge = organizer_challenge.to_string();
                            println!("🎯 Challenge retrieved from organizer peer: {}", challenge);
                            break;
                        }
                    }
                }
            }
            _ => {
                println!("❌ HTTP attempt {} failed", retry_attempt);
            }
        }
        
        // Wait before retry
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }
    
    if challenge.is_empty() {
        return Err("❌ AUTHENTICATION FAILED: Could not retrieve challenge from organizer peer. Please ensure the organizer peer is running and accessible.".into());
    }
    
    // Step 3: Sign challenge and send SubmitResponse command to blockchain
    // NOTE: Keep proxy alive to receive authentication completion!
    println!("✍️ Signing challenge...");
    
    
    let msg = kdapp::pki::to_message(&challenge);
    let signature = kdapp::pki::sign_message(&auth_signer.secret_key(), &msg);
    let signature_hex = hex::encode(signature.0.serialize_der());
    
    println!("📤 Sending SubmitResponse command to blockchain...");
    let auth_command = AuthCommand::SubmitResponse {
        signature: signature_hex,
        nonce: challenge,
    };
    
    let step = EpisodeMessage::<SimpleAuth>::new_signed_command(
        episode_id as u32, 
        auth_command, 
        auth_signer.secret_key(), 
        participant_pubkey
    );
    
    let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, 5000);
    println!("🚀 Submitting SubmitResponse transaction: {}", tx.id());
    
    let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
    
    println!("✅ Authentication commands submitted to Kaspa blockchain!");
    println!("🎯 Real kdapp architecture: Generator → Proxy → Engine → Episode");
    println!("📊 Transactions are now being processed by auth organizer peer's kdapp engine");
    
    // Wait for authentication to complete and get the real session token via HTTP
    println!("⏳ Waiting for authentication completion to retrieve session token...");
    let mut session_token = String::new();
    let mut wait_attempts = 0;
    let max_wait_attempts = 50; // 5 second timeout
    
    'auth_wait: loop {
        wait_attempts += 1;
        
        // Check authentication status via HTTP (organizer peer has the real blockchain state)
        let status_url = format!("{}/auth/status/{}", peer_url, episode_id);
        if let Ok(response) = participant_peer.get(&status_url).send().await {
            if let Ok(status_json) = response.text().await {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&status_json) {
                    if let (Some(authenticated), Some(token)) = (
                        parsed["authenticated"].as_bool(),
                        parsed["session_token"].as_str()
                    ) {
                        if authenticated && !token.is_empty() {
                            session_token = token.to_string();
                            println!("✅ Real session token retrieved from organizer peer: {}", session_token);
                            break 'auth_wait;
                        }
                    }
                }
            }
        }
        
        if wait_attempts >= max_wait_attempts {
            return Err("❌ AUTHENTICATION FAILED: Could not retrieve session token from organizer peer. Authentication incomplete.".into());
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    Ok(AuthenticationResult {
        episode_id,
        session_token,
        authenticated: true,
    })
}