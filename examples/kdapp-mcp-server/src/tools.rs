use crate::state::{ServerState, TicTacToeEpisode, TttCommand};
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use kdapp::episode::{EpisodeId, PayloadMetadata};
use kdapp::engine::EpisodeMessage;
use kdapp::pki::{PubKey};
use kdapp::engine::EngineMsg;
use secp256k1::PublicKey;
use kaspa_consensus_core::Hash;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_addresses::Address;
use kaspa_wrpc_client::prelude::RpcApi;

// Tool: kdapp_start_episode
pub async fn start_episode(state: Arc<ServerState>, participants: Vec<String>) -> Result<String> {
    // Convert string participants to PubKey
    let pubkeys: Vec<PubKey> = participants
        .iter()
        .filter_map(|p| {
            // Parse the hex string into a PublicKey
            // This assumes the participant is a hex-encoded compressed public key (33 bytes)
            if let Ok(pk_bytes) = hex::decode(p) {
                if let Ok(pk) = PublicKey::from_slice(&pk_bytes) {
                    Some(PubKey(pk))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();
    
    // Generate a new episode ID
    // In a real implementation, you would use a more robust method to generate unique IDs
    let episode_id = rand::random::<EpisodeId>();
    
    // Create metadata for the episode creation
    let _metadata = PayloadMetadata {
        accepting_hash: Hash::default(),
        accepting_daa: 0,
        accepting_time: 0,
        tx_id: Hash::default(),
    };
    
    // Create the episode message
    let episode_message = EpisodeMessage::<TicTacToeEpisode>::NewEpisode {
        episode_id,
        participants: pubkeys,
    };
    
    // Send the message to the engine
    // Note: In a real implementation, you would need to handle the response from the engine
    // and possibly wait for the episode to be created before returning the episode ID
    let engine_msg = EngineMsg::BlkAccepted {
        accepting_hash: Hash::default(),
        accepting_daa: 0,
        accepting_time: 0,
        associated_txs: vec![(Hash::default(), borsh::to_vec(&episode_message).unwrap())],
    };
    
    // Send the message to the engine
    state.sender.send(engine_msg).map_err(|e| anyhow::anyhow!("Failed to send message to engine: {}", e))?;
    
    // Return the episode ID
    Ok(episode_id.to_string())
}

// Tool: kdapp_execute_command
pub async fn execute_command(
    state: Arc<ServerState>,
    episode_id: String,
    command: Value,
    _signature: Option<String>,
    signer: Option<String>,
) -> Result<Value> {
    // Parse the episode ID
    let episode_id: EpisodeId = episode_id.parse().map_err(|e| anyhow::anyhow!("Invalid episode ID: {}", e))?;
    
    // Parse the command JSON into a real TttCommand
    let cmd = match command.get("type").and_then(|v| v.as_str()) {
        Some("move") => {
            let row = command.get("row").and_then(|v| v.as_i64()).unwrap_or(-1);
            let col = command.get("col").and_then(|v| v.as_i64()).unwrap_or(-1);
            let player_code = match command.get("player") {
                Some(p) if p.is_string() => {
                    match p.as_str().unwrap().to_ascii_uppercase().as_str() { "X" => 0u8, "O" => 1u8, _ => 255 }
                }
                Some(p) if p.is_u64() => (p.as_u64().unwrap_or(255)) as u8,
                _ => 255,
            };
            if row < 0 || row > 2 || col < 0 || col > 2 || player_code > 1 {
                return Err(anyhow::anyhow!("Invalid move parameters"));
            }
            TttCommand::Move { row: row as u8, col: col as u8, player: player_code }
        }
        _ => return Err(anyhow::anyhow!("Unsupported command type")),
    };
    
    // We will sign commands using the selected agent wallet, so we ignore external signature here.
    
    // Determine which wallet to use (and sign with it)
    let wallet = match signer.as_deref() {
        Some("agent2") => state.agent2_wallet.clone(),
        _ => state.agent1_wallet.clone(), // Default to agent1
    };

    let keypair = wallet.keypair;
    let secret = keypair.secret_key();
    let pubkey = PubKey(keypair.public_key());
    // Prepare the signed message once for engine and optional on-chain tx
    let signed: EpisodeMessage<TicTacToeEpisode> = EpisodeMessage::new_signed_command(episode_id, cmd.clone(), secret.clone(), pubkey);

    // Try to generate and submit a transaction if we have the necessary components
    let transaction_result = if let Some(node_client) = &state.node_client {
        let pattern: kdapp::generator::PatternType = [(0, 0); 10]; // Simple pattern for testing
        let prefix: kdapp::generator::PrefixType = 0x12345678;
        let generator = kdapp::generator::TransactionGenerator::new(keypair, pattern, prefix);

        // Create the participant's Kaspa address from their public key
        let participant_addr = Address::new(
            kaspa_addresses::Prefix::Testnet,
            kaspa_addresses::Version::PubKey,
            &keypair.public_key().serialize()[1..] // Remove compression byte for address
        );
        
        println!("🔑 Signing with pubkey: {}", keypair.public_key());
        println!("🎯 Using participant address: {}", participant_addr);
        
        // Get UTXOs for participant
        match node_client.get_utxos_by_addresses(vec![participant_addr.clone()]).await {
            Ok(entries) => {
                println!("🔍 Found {} UTXO entries for address {}", entries.len(), participant_addr);
                if entries.is_empty() {
                    eprintln!("No UTXOs found for participant wallet. Please fund the wallet at: {}", participant_addr);
                    None
                } else {
                    // Use the first UTXO (in a real implementation, you would want to select an unused UTXO)
                    let entry = &entries[0];
                    println!("📝 Using UTXO: {:?}", entry);
                    let outpoint = TransactionOutpoint::from(entry.outpoint.clone());
                    let utxo_entry = UtxoEntry::from(entry.utxo_entry.clone());
                    let utxo = (outpoint, utxo_entry);
                    
                    // Create a transaction with a higher fee
                    println!("🔨 Building command transaction with higher fee...");
                    let transaction = generator.build_command_transaction(
                        utxo,
                        &participant_addr,
                        &signed,
                        2000, // Increased fee to 2000 to ensure it meets minimum requirements
                    );
                    
                    // Try to submit the transaction
                    println!("📤 Submitting transaction {} to blockchain...", transaction.id());
                    let submit_result = node_client.submit_transaction(transaction.as_ref().into(), false).await;
                    println!("📝 Submit result: {:?}", submit_result);
                    match submit_result {
                        Ok(submit_response) => {
                            let txid = transaction.id().to_string();
                            let url = format!("https://explorer-tn10.kaspa.org/txs/{}", txid);
                            println!("✅ Transaction {} submitted!", txid);
                            println!("🔗 Explorer: {}", url);
                            println!("📄 Submit response: {:?}", submit_response);
                            Some(serde_json::json!({"txid": txid, "explorer_url": url}))
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to submit transaction: {}", e);
                            // Return None if submission fails
                            None
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to get UTXOs: {}", e);
                None
            }
        }
    } else {
        println!("⚠️  Transaction generator or node client not available");
        None
    };
    
    // Always send a signed command to the engine to enforce authorization
    let engine_msg = EngineMsg::BlkAccepted {
        accepting_hash: Hash::default(),
        accepting_daa: 0,
        accepting_time: 0,
        associated_txs: vec![(Hash::default(), borsh::to_vec(&signed).unwrap())],
    };
    
    // Send the message to the engine
    state.sender.send(engine_msg).map_err(|e| anyhow::anyhow!("Failed to send message to engine: {}", e))?;
    
    // Return transaction result or null
    Ok(transaction_result.unwrap_or(serde_json::Value::Null))
}

// Tool: kdapp_get_episode_state
pub async fn get_episode_state(state: Arc<ServerState>, episode_id: String) -> Result<Value> {
    // Parse the episode ID
    let _episode_id: EpisodeId = episode_id.parse().map_err(|e| anyhow::anyhow!("Invalid episode ID: {}", e))?;
    
    // In a real implementation, you would:
    // 1. Get the episode from the engine
    // 2. Serialize its state to JSON
    // 
    // For now, we'll just return a placeholder state
    // In a real implementation, you would need to:
    // - Acquire a read lock on the engine
    // - Look up the episode by ID
    // - Serialize the episode state to JSON
    
    // Look up snapshot from shared state
    if let Ok(ep_id) = episode_id.parse::<kdapp::episode::EpisodeId>() {
        if let Ok(m) = state.ttt_state.lock() {
            if let Some(snap) = m.get(&ep_id) {
                return Ok(serde_json::to_value(snap)?);
            }
        }
    }
    Ok(serde_json::json!({"error":"episode not found"}))
}

// Tool: kdapp_generate_transaction
pub async fn generate_transaction(state: Arc<ServerState>, command: Value) -> Result<Value> {
    // Parse the command into a string
    let _command_str = command.to_string();
    
    // Create a dummy episode message for transaction generation
    let dummy_episode_id = rand::random::<EpisodeId>();
    let episode_message = EpisodeMessage::<TicTacToeEpisode>::UnsignedCommand {
        episode_id: dummy_episode_id,
        cmd: TttCommand::Move { row: 0, col: 0, player: 0 },
    };
    
    // Generate a transaction if we have a transaction generator
    let keypair = state.agent1_wallet.keypair;
    let pattern: kdapp::generator::PatternType = [(0, 0); 10]; // Simple pattern for testing
    let prefix: kdapp::generator::PrefixType = 0x12345678;
    let generator = kdapp::generator::TransactionGenerator::new(keypair, pattern, prefix);

    // Create a dummy UTXO for transaction generation (in a real implementation, you would fetch real UTXOs)
    let dummy_outpoint = TransactionOutpoint::new(Hash::default(), 0);
    let dummy_address = Address::new(
        kaspa_addresses::Prefix::Testnet,
        kaspa_addresses::Version::PubKey,
        &keypair.public_key().serialize()[1..]
    );
    let dummy_utxo = UtxoEntry::new(100000000, kaspa_txscript::pay_to_address_script(&dummy_address), 0, false);
    
    // Create a transaction
        let transaction = generator.build_command_transaction(
            (dummy_outpoint, dummy_utxo),
            &dummy_address,
            &episode_message,
            2000, // Increased fee
        );
    
    // Serialize the transaction to JSON
    let tx_json = serde_json::json!({
        "tx_id": transaction.id().to_string(),
        "transaction": format!("{:?}", transaction)
    });
    
    Ok(tx_json)
}

// Tool: kdapp_get_agent_pubkeys
pub async fn get_agent_pubkeys(state: Arc<ServerState>) -> Result<Value> {
    let a1 = hex::encode(state.agent1_wallet.keypair.public_key().serialize());
    let a2 = hex::encode(state.agent2_wallet.keypair.public_key().serialize());
    Ok(serde_json::json!({
        "agent1_pubkey": a1,
        "agent2_pubkey": a2,
    }))
}
