// src/cli/commands/submit_comment.rs
use crate::core::AuthCommand;
use crate::wallet::KaspaAuthWallet;
use kdapp::{
    engine::{Engine, EpisodeMessage},
    generator::TransactionGenerator,
    proxy::{self, connect_participant_peer},
};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_rpc_core::api::rpc::RpcApi;
use log::info;
use std::sync::mpsc::channel;
use tokio::time::{sleep, Duration};
use crate::core::SimpleAuth;

/// Submit a comment to an episode using participant's own wallet (true kdapp P2P)
pub async fn submit_comment_command(
    episode_id: u64,
    text: String,
    session_token: String,
    private_key: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("💬 Submitting comment to episode {} (P2P kdapp approach)", episode_id);
    
    // Get participant wallet
    let wallet = KaspaAuthWallet::load_for_command("participant-peer", private_key)?;
    
    // Connect to Kaspa network
    let network = NetworkId::with_suffix(NetworkType::Testnet, 10);
    let kaspad = connect_participant_peer(network, None).await?;
    
    // Get participant's address
    let participant_address = Address::new(
        Prefix::Testnet,
        Version::PubKey,
        &wallet.keypair.public_key().serialize()[1..],
    );
    
    println!("🔑 Using participant wallet: {}", participant_address);
    
    // Get UTXOs for the participant
    let entries = kaspad.get_utxos_by_addresses(vec![participant_address.clone()]).await?;
    
    if entries.is_empty() {
        return Err(format!("❌ No UTXOs found for participant address: {}", participant_address).into());
    }
    
    // Create the comment submission command
    let comment_command = AuthCommand::SubmitComment {
        text,
        session_token,
    };
    
    // Create episode message
    let participant_pubkey = kdapp::pki::PubKey::from_secp256k1_public_key(&wallet.keypair.public_key());
    let episode_message = EpisodeMessage::<SimpleAuth>::new_signed_command(
        episode_id as u32,
        comment_command,
        &wallet.keypair.secret_key(),
        participant_pubkey,
    );
    
    // Create transaction generator
    let transaction_generator = TransactionGenerator::new();
    
    // Build transaction
    let utxo = (
        TransactionOutpoint::from(entries[0].outpoint.clone()),
        UtxoEntry::from(entries[0].utxo_entry.clone()),
    );
    
    let transaction = transaction_generator.build_command_transaction(
        utxo,
        &participant_address,
        &episode_message,
        5000, // Fee
    );
    
    println!("📡 Submitting comment transaction to blockchain...");
    
    // Submit transaction
    kaspad.submit_transaction(transaction.as_ref().into(), false).await?;
    
    println!("✅ Comment transaction submitted successfully!");
    println!("📊 Transaction ID: {}", transaction.id());
    println!("🔍 The comment will appear when the transaction is processed by the kdapp engine");
    
    // Wait a moment to see if the transaction gets processed
    println!("⏳ Waiting for transaction processing...");
    sleep(Duration::from_secs(3)).await;
    
    Ok(())
}