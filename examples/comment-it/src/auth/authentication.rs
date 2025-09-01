use crate::core::{AuthWithCommentsEpisode, UnifiedCommand};
use hex;
use secp256k1::Keypair;
use std::error::Error;

#[derive(Debug, Clone)]
pub struct AuthenticationResult {
    pub episode_id: u64,
    pub session_token: String,
    pub authenticated: bool,
}

/// 🚀 Authentication with timeout - wrapper for single authentication attempt
pub async fn run_authentication_with_timeout(
    auth_keypair: Keypair,
    peer_url: String,
    timeout_seconds: u64,
) -> Result<AuthenticationResult, Box<dyn Error>> {
    let kaspa_signer = auth_keypair; // Use same keypair for funding
    let timeout_duration = tokio::time::Duration::from_secs(timeout_seconds);

    match tokio::time::timeout(timeout_duration, run_http_coordinated_authentication(kaspa_signer, auth_keypair, peer_url)).await {
        Ok(result) => result,
        Err(_) => Err(format!("Authentication timeout after {timeout_seconds} seconds").into()),
    }
}

/// 🚀 Full authentication cycle - complete login/logout flow
pub async fn run_full_authentication_cycle(auth_keypair: Keypair, peer_url: String) -> Result<AuthenticationResult, Box<dyn Error>> {
    println!("🔄 Starting full authentication cycle...");

    // Step 1: Authenticate
    let auth_result = run_authentication_with_timeout(auth_keypair, peer_url.clone(), 30).await?;
    println!("✅ Authentication completed - Episode: {}, Session: {}", auth_result.episode_id, auth_result.session_token);

    // Step 2: Verify authentication worked
    if !auth_result.authenticated {
        return Err("Authentication failed".into());
    }

    // Step 3: Revoke session (logout)
    use crate::auth::session::run_session_revocation;
    run_session_revocation(auth_keypair, auth_result.episode_id, auth_result.session_token.clone(), peer_url).await?;
    println!("✅ Session revocation completed");

    Ok(auth_result)
}

/// 🚀 HTTP Coordinated authentication - hybrid kdapp + HTTP coordination  
/// This function attempts to use pure kdapp authentication first, and falls back to HTTP coordination
/// for challenge retrieval if the blockchain-based challenge retrieval times out.
pub async fn run_http_coordinated_authentication(
    kaspa_signer: Keypair,
    auth_signer: Keypair,
    peer_url: String,
) -> Result<AuthenticationResult, Box<dyn Error>> {
    use crate::episode_runner::{AUTH_PATTERN, AUTH_PREFIX};
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_consensus_core::{
        network::NetworkId,
        tx::{TransactionOutpoint, UtxoEntry},
    };
    use kaspa_wrpc_client::prelude::RpcApi;
    use kdapp::{
        engine::EpisodeMessage,
        generator::{self, TransactionGenerator},
        proxy::connect_client,
    };

    let client_pubkey = kdapp::pki::PubKey(auth_signer.public_key());
    println!("🔑 Auth public key: {client_pubkey}");

    // Connect to Kaspa network (real blockchain!)
    let network = NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 10);
    println!("📡 Connecting to testnet-10 blockchain...");

    let kaspad = connect_client(network, None).await?;

    // Create Kaspa address for funding transactions
    let kaspa_addr = Address::new(Prefix::Testnet, Version::PubKey, &kaspa_signer.x_only_public_key().0.serialize());
    println!("💰 Kaspa address: {kaspa_addr}");

    // Get UTXOs for transaction funding
    println!("🔍 Fetching UTXOs...");
    let entries = kaspad.get_utxos_by_addresses(vec![kaspa_addr.clone()]).await?;

    if entries.is_empty() {
        return Err("No UTXOs found! Please fund the Kaspa address first.".into());
    }

    let mut utxo =
        entries.first().map(|entry| (TransactionOutpoint::from(entry.outpoint), UtxoEntry::from(entry.utxo_entry.clone()))).unwrap();

    println!("✅ UTXO found: {}", utxo.0);

    // Create real transaction generator (kdapp architecture!)
    let generator = TransactionGenerator::new(kaspa_signer, AUTH_PATTERN, AUTH_PREFIX);

    // Step 1: Request server to create and manage the authentication episode
    // The organizer peer creates episodes so its kdapp engine knows about them
    println!("🔗 Requesting organizer peer to create authentication episode...");

    let client = reqwest::Client::new();
    let public_key_hex = hex::encode(client_pubkey.0.serialize());

    // Use the /auth/start endpoint which creates episodes on the server side
    let start_url = format!("{peer_url}/auth/start");
    let start_response = client
        .post(&start_url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "public_key": public_key_hex
        }))
        .send()
        .await?;

    let start_data: serde_json::Value = start_response.json().await?;
    let episode_id = start_data["episode_id"].as_u64().ok_or("Server did not return valid episode_id")?;

    println!("✅ Authentication episode {episode_id} created by organizer peer");

    // Step 2: Send RequestChallenge command to blockchain
    println!("📨 Sending RequestChallenge command to blockchain...");

    let auth_command = UnifiedCommand::RequestChallenge;
    let step = EpisodeMessage::<AuthWithCommentsEpisode>::new_signed_command(
        episode_id as u32,
        auth_command,
        auth_signer.secret_key(),
        client_pubkey,
    );

    let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, 5000);
    println!("🚀 Submitting RequestChallenge transaction: {}", tx.id());

    let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
    utxo = generator::get_first_output_utxo(&tx);

    println!("✅ RequestChallenge transaction submitted to blockchain!");
    println!("🔗 [ VERIFY ON KASPA EXPLORER → ] https://explorer-tn10.kaspa.org/txs/{}", tx.id());
    println!("🔗 [ VIEW WALLET ON EXPLORER → ] https://explorer-tn10.kaspa.org/addresses/{kaspa_addr}");
    println!("⏳ Waiting for challenge response from auth server...");

    // Wait for server to process RequestChallenge and generate challenge
    println!("⏳ Waiting for server to generate challenge...");
    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

    let mut challenge = String::new();
    let client = reqwest::Client::new();

    // Get challenge via HTTP (polling until available)
    for retry_attempt in 1..=10 {
        println!("🔄 Checking for challenge attempt {retry_attempt} of 10...");

        let status_url = format!("{peer_url}/auth/status/{episode_id}");

        match client.get(&status_url).send().await {
            Ok(response) if response.status().is_success() => {
                if let Ok(status_json) = response.text().await {
                    println!("📡 HTTP status response: {status_json}");
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&status_json) {
                        if let Some(server_challenge) = parsed["challenge"].as_str() {
                            challenge = server_challenge.to_string();
                            println!("🎯 Challenge retrieved from server: {challenge}");
                            break;
                        }
                    }
                }
            }
            _ => {
                println!("❌ HTTP attempt {retry_attempt} failed");
            }
        }

        // Wait before retry
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    if challenge.is_empty() {
        return Err("❌ AUTHENTICATION FAILED: Could not retrieve challenge from server. Please ensure the organizer peer is running and accessible.".into());
    }

    // Step 3: Sign challenge and send SubmitResponse command to blockchain
    // NOTE: Keep proxy alive to receive authentication completion!
    println!("✍️ Signing challenge...");

    let msg = kdapp::pki::to_message(&challenge);
    let signature = kdapp::pki::sign_message(&auth_signer.secret_key(), &msg);
    let signature_hex = hex::encode(signature.0.serialize_der());

    println!("📤 Sending SubmitResponse command to blockchain...");
    let auth_command = UnifiedCommand::SubmitResponse { signature: signature_hex, nonce: challenge };

    let step = EpisodeMessage::<AuthWithCommentsEpisode>::new_signed_command(
        episode_id as u32,
        auth_command,
        auth_signer.secret_key(),
        client_pubkey,
    );

    let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, 5000);
    println!("🚀 Submitting SubmitResponse transaction: {}", tx.id());

    let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;

    println!("✅ Authentication commands submitted to Kaspa blockchain!");
    println!("🔗 [ VERIFY ON KASPA EXPLORER → ] https://explorer-tn10.kaspa.org/txs/{}", tx.id());
    println!("🔗 [ VIEW WALLET ON EXPLORER → ] https://explorer-tn10.kaspa.org/addresses/{kaspa_addr}");
    println!("🎯 Real kdapp architecture: Generator → Proxy → Engine → Episode");
    println!("📊 Transactions submitted to Kaspa blockchain - organizer peer will detect and respond");

    // Wait for authentication to complete and get the real session token via HTTP
    println!("⏳ Waiting for authentication completion to retrieve session token...");
    let mut wait_attempts = 0;
    let max_wait_attempts = 50; // 5 second timeout

    let session_token = loop {
        wait_attempts += 1;

        // Check authentication status via HTTP (server has the real blockchain state)
        let status_url = format!("{peer_url}/auth/status/{episode_id}");
        if let Ok(response) = client.get(&status_url).send().await {
            if let Ok(status_json) = response.text().await {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&status_json) {
                    if let (Some(authenticated), Some(token)) = (parsed["authenticated"].as_bool(), parsed["session_token"].as_str()) {
                        if authenticated && !token.is_empty() {
                            let session_token = token.to_string();
                            println!("✅ Real session token retrieved from server: {session_token}");
                            break session_token;
                        }
                    }
                }
            }
        }

        if wait_attempts >= max_wait_attempts {
            break "".to_string(); // Return empty string, handle error after loop
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    };

    // Check if authentication timed out
    if session_token.is_empty() {
        return Err("❌ AUTHENTICATION FAILED: Could not retrieve session token from server. Authentication incomplete.".into());
    }

    Ok(AuthenticationResult { episode_id, session_token, authenticated: true })
}
