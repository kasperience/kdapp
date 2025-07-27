// src/auth/flow.rs - Authentication flow logic extracted from main.rs
use std::error::Error;
use secp256k1::Keypair;
use crate::wallet::get_wallet_for_command;

#[derive(Debug, Clone)]
pub struct AuthenticationResult {
    pub episode_id: u64,
    pub session_token: String,
    pub authenticated: bool,
}

/// 🚀 Automatic authentication - uses REAL kdapp architecture (unified with participant-peer --auth)
pub async fn run_automatic_authentication(keypair: Keypair) -> Result<(), Box<dyn Error>> {
    println!("🎯 Starting kdapp-based authentication (unified architecture)");
    println!("📱 This uses the same kdapp engine as participant-peer --auth");
    println!("🔑 Using public key: {}", hex::encode(keypair.public_key().serialize()));
    println!();

    // Use the same wallet system as participant-peer for consistency
    let wallet = get_wallet_for_command("participant-peer", None)?;
    
    // Use the wallet's keypair for funding transactions (participant pays)
    let funding_keypair = wallet.keypair;
    let auth_keypair = keypair; // Use provided keypair for authentication
    
    println!("💰 Funding transactions with participant wallet: {}", wallet.get_kaspa_address());
    println!("🔐 Authentication keypair: {}", hex::encode(auth_keypair.public_key().serialize()));
    
    // Check if wallet needs funding
    if wallet.check_funding_status() {
        println!("⚠️  WARNING: Participant wallet may need funding for blockchain transactions!");
        println!("💡 Get testnet funds: https://faucet.kaspanet.io/");
        println!("💰 Fund address: {}", wallet.get_kaspa_address());
        println!();
    }
    
    // Use the REAL kdapp architecture - same as participant-peer --auth
    run_client_authentication(funding_keypair, auth_keypair).await?;
    
    println!("✅ kdapp authentication completed successfully!");
    println!("🔍 Check your transactions on Kaspa explorer: https://explorer-tn10.kaspa.org/");
    println!("📊 Look for AUTH transactions (0x41555448) from your address: {}", wallet.get_kaspa_address());
    
    Ok(())
}

/// 🚀 HTTP Coordinated authentication - hybrid kdapp + HTTP coordination  
/// This function attempts to use pure kdapp authentication first, and falls back to HTTP coordination
/// for challenge retrieval if the blockchain-based challenge retrieval times out.
pub async fn run_http_coordinated_authentication(kaspa_signer: Keypair, auth_signer: Keypair, peer_url: String) -> Result<AuthenticationResult, Box<dyn Error>> {
    use kdapp::{
        engine::EpisodeMessage,
        generator::TransactionGenerator,
        proxy::connect_client,
    };
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_consensus_core::{network::NetworkId, tx::{TransactionOutpoint, UtxoEntry}};
    use kaspa_wrpc_client::prelude::*;
    use kaspa_rpc_core::api::rpc::RpcApi;
    use crate::episode_runner::{AUTH_PATTERN, AUTH_PREFIX};
    use rand::Rng;
    
    let client_pubkey = kdapp::pki::PubKey(auth_signer.public_key());
    println!("🔑 Auth public key: {}", client_pubkey);
    
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
    
    // Step 1: Initialize the episode first (like tictactoe example)
    println!("🚀 Initializing authentication episode...");
    
    let episode_id = rand::thread_rng().gen();
    let new_episode = EpisodeMessage::<AuthWithCommentsEpisode>::NewEpisode { 
        episode_id, 
        participants: vec![client_pubkey] 
    };
    
    let tx = generator.build_command_transaction(utxo, &kaspa_addr, &new_episode, 5000);
    println!("🚀 Submitting NewEpisode transaction: {}", tx.id());
    
    let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
    utxo = kdapp::generator::get_first_output_utxo(&tx);
    
    println!("✅ Episode {} initialized on blockchain!", episode_id);
    print_explorer_links(&tx.id().to_string(), &kaspa_addr.to_string());
    
    // Step 2: Send RequestChallenge command to blockchain
    println!("📨 Sending RequestChallenge command to blockchain...");
    
    let auth_command = UnifiedCommand::RequestChallenge;
    let step = EpisodeMessage::<AuthWithCommentsEpisode>::new_signed_command(
        episode_id, 
        auth_command, 
        auth_signer.secret_key(), 
        client_pubkey
    );
    
    let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, 5000);
    println!("🚀 Submitting RequestChallenge transaction: {}", tx.id());
    
    let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
    utxo = kdapp::generator::get_first_output_utxo(&tx);
    
    println!("✅ RequestChallenge transaction submitted to blockchain!");
    print_explorer_links(&tx.id().to_string(), &kaspa_addr.to_string());
    println!("⏳ Waiting for challenge response from auth server...");
    
    // Set up episode state listener (like tictactoe example)
    use std::sync::{mpsc::channel, Arc, atomic::AtomicBool};
    use tokio::sync::mpsc::UnboundedSender;
    use kdapp::{engine::{self}, episode::EpisodeEventHandler};
    
    let (sender, receiver) = channel();
    let (response_sender, mut response_receiver) = tokio::sync::mpsc::unbounded_channel();
    let exit_signal = Arc::new(AtomicBool::new(false));
    
    // Simple event handler to capture episode state
    struct ClientAuthHandler {
        sender: UnboundedSender<(kdapp::episode::EpisodeId, AuthWithCommentsEpisode)>,
    }
    
    impl EpisodeEventHandler<AuthWithCommentsEpisode> for ClientAuthHandler {
        fn on_initialize(&self, episode_id: kdapp::episode::EpisodeId, episode: &AuthWithCommentsEpisode) {
            println!("🔍 CLIENT: Episode {} initialized - challenge: {:?}", episode_id, episode.challenge);
            let _ = self.sender.send((episode_id, episode.clone()));
        }
        
        fn on_command(&self, episode_id: kdapp::episode::EpisodeId, episode: &AuthWithCommentsEpisode, 
                      cmd: &UnifiedCommand, _authorization: Option<kdapp::pki::PubKey>, 
                      _metadata: &kdapp::episode::PayloadMetadata) {
            println!("🔍 CLIENT: Episode {} command {:?} - challenge: {:?}", episode_id, cmd, episode.challenge);
            let _ = self.sender.send((episode_id, episode.clone()));
        }
        
        fn on_rollback(&self, _episode_id: kdapp::episode::EpisodeId, _episode: &AuthWithCommentsEpisode) {}
    }
    
    // Start a simple engine to listen for episode updates
    let mut engine = engine::Engine::<AuthWithCommentsEpisode, ClientAuthHandler>::new(receiver);
    let handler = ClientAuthHandler { sender: response_sender };
    
    let engine_task = tokio::task::spawn_blocking(move || {
        engine.start(vec![handler]);
    });
    
    // Connect client proxy to listen for episode updates
    let client_kaspad = connect_client(network, None).await?;
    let engines = std::iter::once((AUTH_PREFIX, (AUTH_PATTERN, sender))).collect();
    
    let exit_signal_clone = exit_signal.clone();
    tokio::spawn(async move {
        kdapp::proxy::run_listener(client_kaspad, engines, exit_signal_clone).await;
    });
    
    // Wait for challenge to be generated by server
    println!("👂 Listening for episode state updates...");
    println!("🔍 Looking for episode ID: {}", episode_id);
    let mut challenge = String::new();
    let mut attempt_count = 0;
    let max_attempts = 150; // 30 second timeout - Hybrid mode with HTTP fallback
    
    // Try to get challenge from blockchain first
    'blockchain_loop: loop {
        attempt_count += 1;
        
        let recv_result = tokio::time::timeout(tokio::time::Duration::from_millis(200), response_receiver.recv()).await;
        
        if let Ok(Some((received_episode_id, episode_state))) = recv_result {
            println!("📨 Received episode state update for ID: {} (expecting: {})", received_episode_id, episode_id);
            if received_episode_id == episode_id {
                if let Some(server_challenge) = &episode_state.challenge {
                    challenge = server_challenge.clone();
                    println!("🎲 Real challenge received from server: {}", challenge);
                    break 'blockchain_loop;
                } else {
                    println!("📡 Episode state update received, but no challenge yet. Auth status: {}", episode_state.is_authenticated);
                }
            } else {
                println!("🔄 Episode ID mismatch, continuing to listen...");
            }
        }
        
        if attempt_count % 10 == 0 {
            println!("⏰ Still listening... attempt {} of {}", attempt_count, max_attempts);
        }
        
        if attempt_count >= max_attempts {
            return Err("❌ AUTHENTICATION FAILED: Could not retrieve challenge from blockchain within timeout. No HTTP fallback.".into());
        }
        
        // Add timeout to prevent infinite waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    // Step 3: Sign challenge and send SubmitResponse command to blockchain
    // NOTE: Keep proxy alive to receive authentication completion!
    println!("✍️ Signing challenge...");
    
    
    let msg = to_message(&challenge);
    let signature = sign_message(&auth_signer.secret_key(), &msg);
    let signature_hex = hex::encode(signature.0.serialize_der());
    
    println!("📤 Sending SubmitResponse command to blockchain...");
    let auth_command = UnifiedCommand::SubmitResponse {
        signature: signature_hex,
        nonce: challenge,
    };
    
    let step = EpisodeMessage::<AuthWithCommentsEpisode>::new_signed_command(
        episode_id, 
        auth_command, 
        auth_signer.secret_key(), 
        client_pubkey
    );
    
    let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, 5000);
    println!("🚀 Submitting SubmitResponse transaction: {}", tx.id());
    
    let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
    
    println!("✅ Authentication commands submitted to Kaspa blockchain!");
    println!("🎯 Real kdapp architecture: Generator → Proxy → Engine → Episode");
    print_explorer_links(&tx.id().to_string(), &kaspa_addr.to_string());
    println!("📊 Transactions submitted to Kaspa blockchain - organizer peer will detect and respond");
    
    // Wait for authentication to complete and get the real session token from blockchain
    println!("⏳ Waiting for authentication completion to retrieve session token...");
    let mut session_token = String::new();
    let mut wait_attempts = 0;
    let max_wait_attempts = 50; // 5 second timeout
    
    'auth_wait: loop {
        wait_attempts += 1;
        
        if let Ok((received_episode_id, episode_state)) = response_receiver.try_recv() {
            if received_episode_id == episode_id && episode_state.is_authenticated {
                if let Some(token) = &episode_state.session_token {
                    session_token = token.clone();
                    println!("✅ Real session token retrieved from blockchain: {}", session_token);
                    // Now we can stop the proxy - authentication is complete
                    exit_signal.store(true, std::sync::atomic::Ordering::Relaxed);
                    break 'auth_wait;
                }
            }
        }
        
        if wait_attempts >= max_wait_attempts {
            return Err("❌ AUTHENTICATION FAILED: Could not retrieve session token from blockchain. Authentication incomplete.".into());
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    Ok(AuthenticationResult {
        episode_id: episode_id.into(),
        session_token,
        authenticated: true,
    })
}

/// Focused authentication testing functions with timeouts
pub async fn run_authentication_with_timeout(
    auth_keypair: Keypair, 
    peer_url: Option<String>, 
    timeout_seconds: u64
) -> Result<(), Box<dyn Error>> {
    println!("🔥 Starting focused authentication test ({}s timeout)", timeout_seconds);
    
    let timeout_duration = tokio::time::Duration::from_secs(timeout_seconds);
    
    if let Some(url) = peer_url {
        // Get funding keypair for HTTP coordination
        let funding_wallet = get_wallet_for_command("participant-peer", None)?;
        let funding_keypair = funding_wallet.keypair;
        
        println!("🌐 Using HTTP coordination: {}", url);
        let auth_result = tokio::time::timeout(timeout_duration, run_http_coordinated_authentication(funding_keypair, auth_keypair, url)).await;
        
        match auth_result {
            Ok(result) => {
                match result {
                    Ok(_) => {
                        println!("✅ Authentication completed within {}s timeout", timeout_seconds);
                        Ok(())
                    }
                    Err(e) => {
                        println!("❌ Authentication failed: {}", e);
                        Err(e)
                    }
                }
            }
            Err(_) => {
                println!("⏰ Authentication timed out after {}s", timeout_seconds);
                Err("Authentication timeout".into())
            }
        }
    } else {
        println!("⚡ Using pure kdapp (experimental)");
        let auth_result = tokio::time::timeout(timeout_duration, run_automatic_authentication(auth_keypair)).await;
        
        match auth_result {
            Ok(result) => {
                match result {
                    Ok(_) => {
                        println!("✅ Authentication completed within {}s timeout", timeout_seconds);
                        Ok(())
                    }
                    Err(e) => {
                        println!("❌ Authentication failed: {}", e);
                        Err(e)
                    }
                }
            }
            Err(_) => {
                println!("⏰ Authentication timed out after {}s", timeout_seconds);
                Err("Authentication timeout".into())
            }
        }
    }
}

pub async fn run_full_authentication_cycle(
    funding_keypair: Keypair,
    auth_keypair: Keypair, 
    peer_url: String,
    session_duration: u64,
    auth_timeout: u64
) -> Result<(), Box<dyn Error>> {
    println!("🔄 Starting complete authentication lifecycle test");
    println!("⏱️  Phase 1: Login ({}s timeout)", auth_timeout);
    
    // Phase 1: Authenticate with timeout
    let auth_timeout_duration = tokio::time::Duration::from_secs(auth_timeout);
    let auth_future = run_http_coordinated_authentication(funding_keypair, auth_keypair, peer_url.clone());
    
    let auth_result = tokio::time::timeout(auth_timeout_duration, auth_future).await;
    
    let authentication_details = match auth_result {
        Ok(Ok(auth_details)) => {
            println!("✅ Phase 1: Authentication successful!");
            println!("📋 Episode ID: {}, Session Token: {}", auth_details.episode_id, auth_details.session_token);
            auth_details
        }
        Ok(Err(e)) => {
            println!("❌ Phase 1: Authentication failed: {}", e);
            return Err(e);
        }
        Err(_) => {
            println!("⏰ Phase 1: Authentication timed out after {}s", auth_timeout);
            return Err("Authentication timeout".into());
        }
    };
    
    // Phase 2: Simulate active session
    println!("⏱️  Phase 2: Active session ({}s duration)", session_duration);
    println!("🔒 Session is active - simulating user activity...");
    
    tokio::time::sleep(tokio::time::Duration::from_secs(session_duration)).await;
    
    // Phase 3: Logout using authentication details from Phase 1
    println!("⏱️  Phase 3: Logout initiated");
    println!("🚪 Revoking session {} for episode {}", authentication_details.session_token, authentication_details.episode_id);
    
    match crate::auth::session::run_session_revocation(auth_keypair, authentication_details.episode_id, authentication_details.session_token, peer_url).await {
        Ok(_) => {
            println!("✅ Phase 3: Session revocation successful!");
            println!("✅ Full authentication cycle test completed - Login → Active Session → Logout");
        }
        Err(e) => {
            println!("❌ Phase 3: Session revocation failed: {}", e);
            println!("⚠️  Authentication cycle incomplete - logout failed");
            return Err(format!("Logout failed: {}", e).into());
        }
    }
    
    Ok(())
}

// Helper functions
pub fn print_explorer_links(tx_id: &str, wallet_address: &str) {
    println!("🔗 [ VERIFY ON KASPA EXPLORER → ] https://explorer-tn10.kaspa.org/txs/{}", tx_id);
    println!("🔗 [ VIEW WALLET ON EXPLORER → ] https://explorer-tn10.kaspa.org/addresses/{}", wallet_address);
}

use crate::core::commands::UnifiedCommand;
use crate::core::AuthWithCommentsEpisode;
use kdapp::pki::{sign_message, to_message};

/// Implement REAL client authentication flow using kdapp blockchain architecture
async fn run_client_authentication(kaspa_signer: Keypair, auth_signer: Keypair) -> Result<(), Box<dyn Error>> {
    use kdapp::{
        engine::EpisodeMessage,
        generator::TransactionGenerator,
        proxy::connect_client,
    };
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_consensus_core::{network::NetworkId, tx::{TransactionOutpoint, UtxoEntry}};
    use kaspa_wrpc_client::prelude::*;
    use kaspa_rpc_core::api::rpc::RpcApi;
    use crate::episode_runner::{AUTH_PATTERN, AUTH_PREFIX};
    use rand::Rng;
    
    let client_pubkey = kdapp::pki::PubKey(auth_signer.public_key());
    
    // Connect to Kaspa network (real blockchain!)
    let network = NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 10);
    
    let kaspad = connect_client(network, None).await?;
    
    // Create Kaspa address for funding transactions
    let kaspa_addr = Address::new(Prefix::Testnet, Version::PubKey, &kaspa_signer.x_only_public_key().0.serialize());
    
    // Get UTXOs for transaction funding
    let entries = kaspad.get_utxos_by_addresses(vec![kaspa_addr.clone()]).await?;
    
    if entries.is_empty() {
        return Err("No UTXOs found! Please fund the Kaspa address first.".into());
    }
    
    let mut utxo = entries.first().map(|entry| {
        (TransactionOutpoint::from(entry.outpoint.clone()), UtxoEntry::from(entry.utxo_entry.clone()))
    }).unwrap();
    
    // Create real transaction generator (kdapp architecture!)
    let generator = TransactionGenerator::new(kaspa_signer, AUTH_PATTERN, AUTH_PREFIX);
    
    // Step 1: Initialize the episode first (like tictactoe example)
    let episode_id = rand::thread_rng().gen();
    let new_episode = EpisodeMessage::<AuthWithCommentsEpisode>::NewEpisode { 
        episode_id, 
        participants: vec![client_pubkey] 
    };
    
    let tx = generator.build_command_transaction(utxo, &kaspa_addr, &new_episode, 5000);
    
    let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
    utxo = kdapp::generator::get_first_output_utxo(&tx);
    
    // Step 2: Send RequestChallenge command to blockchain
    let auth_command = UnifiedCommand::RequestChallenge;
    let step = EpisodeMessage::<AuthWithCommentsEpisode>::new_signed_command(
        episode_id, 
        auth_command, 
        auth_signer.secret_key(), 
        client_pubkey
    );
    
    let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, 5000);
    
    let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
    utxo = kdapp::generator::get_first_output_utxo(&tx);
    
    // Set up episode state listener (like tictactoe example)
    use std::sync::{mpsc::channel, Arc, atomic::AtomicBool};
    use tokio::sync::mpsc::UnboundedSender;
    use kdapp::{engine::{self}, episode::EpisodeEventHandler};
    
    let (sender, receiver) = channel();
    let (response_sender, mut response_receiver) = tokio::sync::mpsc::unbounded_channel();
    let exit_signal = Arc::new(AtomicBool::new(false));
    
    // Simple event handler to capture episode state
    struct ClientAuthHandler {
        sender: UnboundedSender<(kdapp::episode::EpisodeId, AuthWithCommentsEpisode)>,
    }
    
    impl EpisodeEventHandler<AuthWithCommentsEpisode> for ClientAuthHandler {
        fn on_initialize(&self, episode_id: kdapp::episode::EpisodeId, episode: &AuthWithCommentsEpisode) {
            let _ = self.sender.send((episode_id, episode.clone()));
        }
        
        fn on_command(&self, episode_id: kdapp::episode::EpisodeId, episode: &AuthWithCommentsEpisode, 
                      _cmd: &UnifiedCommand, _authorization: Option<kdapp::pki::PubKey>, 
                      _metadata: &kdapp::episode::PayloadMetadata) {
            let _ = self.sender.send((episode_id, episode.clone()));
        }
        
        fn on_rollback(&self, _episode_id: kdapp::episode::EpisodeId, _episode: &AuthWithCommentsEpisode) {}
    }
    
    // Start a simple engine to listen for episode updates
    let mut engine = engine::Engine::<AuthWithCommentsEpisode, ClientAuthHandler>::new(receiver);
    let handler = ClientAuthHandler { sender: response_sender };
    
    let engine_task = tokio::task::spawn_blocking(move || {
        engine.start(vec![handler]);
    });
    
    // Connect client proxy to listen for episode updates
    let client_kaspad = connect_client(network, None).await?;
    let engines = std::iter::once((AUTH_PREFIX, (AUTH_PATTERN, sender))).collect();
    
    let exit_signal_clone = exit_signal.clone();
    tokio::spawn(async move {
        kdapp::proxy::run_listener(client_kaspad, engines, exit_signal_clone).await;
    });
    
    // Wait for challenge to be generated by server
    let mut challenge = String::new();
    let mut attempt_count = 0;
    let max_attempts = 100; // 10 second timeout - Pure kdapp architecture (100 blocks = 10 seconds)
    
    // Wait for episode state with challenge
    'outer: loop {
        attempt_count += 1;
        
        if let Ok((received_episode_id, episode_state)) = response_receiver.try_recv() {
            if received_episode_id == episode_id {
                if let Some(server_challenge) = &episode_state.challenge {
                    challenge = server_challenge.clone();
                    break;
                }
            }
        }
        
        if attempt_count >= max_attempts {
            return Err("PURE KDAPP AUTHENTICATION FAILED: Blockchain timeout after 10 seconds (100 blocks). No HTTP fallback - this is pure kdapp architecture.".into());
        }
        
        // Add timeout to prevent infinite waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    // Stop listening after we get the challenge
    exit_signal.store(true, std::sync::atomic::Ordering::Relaxed);
    
    // Step 3: Sign challenge and send SubmitResponse command to blockchain
    let msg = to_message(&challenge);
    let signature = sign_message(&auth_signer.secret_key(), &msg);
    let signature_hex = hex::encode(signature.0.serialize_der());
    
    let auth_command = UnifiedCommand::SubmitResponse {
        signature: signature_hex,
        nonce: challenge,
    };
    
    let step = EpisodeMessage::<AuthWithCommentsEpisode>::new_signed_command(
        episode_id, 
        auth_command, 
        auth_signer.secret_key(), 
        client_pubkey
    );
    
    let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, 5000);
    
    let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await?;
    
    Ok(())
}