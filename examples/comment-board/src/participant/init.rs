use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_wrpc_client::prelude::*;
use kdapp::{engine::EpisodeMessage, episode::EpisodeId, generator};
use log::*;
use rand::Rng;
use secp256k1::Keypair;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    episode::board_with_contract::{ContractCommentBoard, ContractState},
    utils::{FEE, PATTERN, PREFIX},
    wallet::UtxoLockManager,
};

pub struct ParticipantInitState {
    pub utxo_manager: UtxoLockManager,
    pub generator: generator::TransactionGenerator,
    pub episode_id: EpisodeId,
    pub _received_episode_id: EpisodeId,
    pub board_state: ContractState,
    pub utxo: (TransactionOutpoint, UtxoEntry),
}

pub async fn initialize_participant(
    kaspad: &KaspaRpcClient,
    kaspa_signer: Keypair,
    kaspa_addr: Address,
    mut response_receiver: UnboundedReceiver<(EpisodeId, ContractState)>,
    target_episode_id: Option<u32>,
) -> Result<(ParticipantInitState, UnboundedReceiver<(EpisodeId, ContractState)>), Box<dyn std::error::Error>> {
    let entries = kaspad.get_utxos_by_addresses(vec![kaspa_addr.clone()]).await?;
    if entries.is_empty() {
        println!("❌ No funds found in wallet!");
        println!("💰 Your address: {kaspa_addr}");
        println!("🚰 Get testnet KAS from: https://faucet.kaspanet.io/");
        println!("🔗 Check balance: https://explorer-tn10.kaspa.org/addresses/{kaspa_addr}");
        println!("⏳ Wait 1-2 minutes after requesting from faucet, then try again");
        return Err("No funds found".into());
    }
    let entry = entries.first().cloned().ok_or("No UTXO entries found")?;
    let mut utxo = (TransactionOutpoint::from(entry.outpoint), UtxoEntry::from(entry.utxo_entry));

    let mut utxo_manager = UtxoLockManager::new(kaspad, kaspa_addr.clone(), kaspa_signer).await?;
    info!("🏦 Wallet initialized with {:.6} KAS available", utxo_manager.get_available_balance() as f64 / 100_000_000.0);

    let max_safe_utxo = 100_000;
    let target_chunk_size = 50_000;
    println!("🔄 Ensuring mass-safe micro-UTXOs (this may take a few seconds)...");
    if let Err(e) = utxo_manager.ensure_micro_utxos(10, max_safe_utxo, target_chunk_size).await {
        println!("⚠️ Warning: Could not prepare micro-UTXOs automatically: {e}");
        println!("💡 Manual workaround: send multiple small amounts (< 0.001 KAS each) to your wallet");
    }

    if let Err(e) = utxo_manager.refresh_utxos().await {
        warn!("Failed to refresh UTXOs: {e}");
    }
    if let Some((new_outpoint, new_entry)) = utxo_manager.available_utxos.first() {
        utxo = (*new_outpoint, new_entry.clone());
    }

    let generator = generator::TransactionGenerator::new(kaspa_signer, PATTERN, PREFIX);

    let episode_id = if let Some(room_id) = target_episode_id {
        println!("🎯 Joining room with Episode ID: {room_id}");
        println!("🔧 Registering episode with local engine for command processing...");
        println!("💰 You pay for your own comments with address: {kaspa_addr}");

        let register_episode = EpisodeMessage::<ContractCommentBoard>::NewEpisode { episode_id: room_id, participants: vec![] };
        let tx = generator.build_command_transaction(utxo.clone(), &kaspa_addr, &register_episode, FEE);
        info!("Submitting episode registration for room {room_id}: {}", tx.id());
        crate::utils::submit_tx_retry(kaspad, tx.as_ref(), 3).await.map_err(|e| e.to_string())?;
        utxo = generator::get_first_output_utxo(&tx);
        room_id
    } else {
        let new_episode_id = rand::thread_rng().gen();
        println!("🚀 Creating new room with Episode ID: {new_episode_id}");
        println!("📢 Share this Episode ID with friends to let them join!");
        println!("⚠️  IMPORTANT: Friends must start their terminals BEFORE you create this room!");
        println!("💰 You pay for room creation with address: {kaspa_addr}");

        let new_episode = EpisodeMessage::<ContractCommentBoard>::NewEpisode { episode_id: new_episode_id, participants: vec![] };
        let tx = generator.build_command_transaction(utxo.clone(), &kaspa_addr, &new_episode, FEE);
        info!("Submitting room creation: {}", tx.id());
        crate::utils::submit_tx_retry(kaspad, tx.as_ref(), 3).await.map_err(|e| e.to_string())?;
        utxo = generator::get_first_output_utxo(&tx);
        new_episode_id
    };

    let (received_episode_id, board_state) = response_receiver.recv().await.ok_or("Failed to receive initial episode state")?;
    println!("📺 Connected to room: Episode {received_episode_id}");

    println!("=== 💬 Comment Board ===");
    println!("Comments: {} | Members: {}", board_state.comments.len(), board_state.room_members.len());
    for comment in &board_state.comments {
        println!("[{}] {}: {}", comment.timestamp, &comment.author[..8], comment.text);
    }
    println!("========================");

    let init_state =
        ParticipantInitState { utxo_manager, generator, episode_id, _received_episode_id: received_episode_id, board_state, utxo };

    Ok((init_state, response_receiver))
}
