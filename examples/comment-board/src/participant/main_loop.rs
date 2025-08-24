use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{timeout, Duration};

use crate::{
    cli::Args,
    episode::board_with_contract::{ContractCommentBoard, ContractState},
    participant::{auth, commands, init},
    utils::{PATTERN, PREFIX},
};
use kdapp::episode::EpisodeId;
use kdapp::pki::PubKey;
use secp256k1::{Keypair, SecretKey};

use kaspa_addresses::Address;
use kaspa_wrpc_client::prelude::{KaspaRpcClient, RpcApi};

use kdapp::engine::EpisodeMessage;

use crate::episode::commands::ContractCommand;

pub async fn run_comment_board(
    kaspad: KaspaRpcClient,
    kaspa_signer: Keypair,
    kaspa_addr: Address,
    response_receiver: UnboundedReceiver<(EpisodeId, ContractState)>,
    exit_signal: Arc<AtomicBool>,
    participant_sk: SecretKey,
    participant_pk: PubKey,
    target_episode_id: Option<u32>,
    args: Args,
) {
    // 1. Initialize
    let (mut init_state, mut response_receiver) = match init::initialize_participant(&kaspad, kaspa_signer, kaspa_addr.clone(), response_receiver, target_episode_id).await {
        Ok(state) => state,
        Err(e) => {
            println!("❌ Initialization failed: {}", e);
            return;
        }
    };

    // 2. Authenticate
    let (mut board_state, new_response_receiver) = match auth::perform_authentication(
        &kaspad,
        &init_state.generator,
        init_state.board_state,
        response_receiver,
        &exit_signal,
        participant_sk,
        participant_pk,
        init_state.episode_id,
        &mut init_state.utxo,
        &kaspa_addr,
    ).await {
        Ok(state) => state,
        Err(e) => {
            println!("❌ Authentication failed: {}", e);
            return;
        }
    };
    response_receiver = new_response_receiver;

    // 3. Auto-join room after authentication to populate member count
    {
        let join_cmd = ContractCommand::JoinRoom { bond_amount: 0 };
        let join_step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(
            init_state.episode_id,
            join_cmd,
            participant_sk,
            participant_pk,
        );
        let tx = init_state
            .generator
            .build_command_transaction(init_state.utxo.clone(), &kaspa_addr, &join_step, crate::utils::FEE);
        let _ = crate::utils::submit_tx_retry(&kaspad, tx.as_ref(), 3).await;
        init_state.utxo = kdapp::generator::get_first_output_utxo(&tx);
    }

    // 4. Main Loop
    let mut input = String::new();
    loop {
        // Before rendering, block briefly to catch the next incoming state if it's in-flight
        if let Ok(Some((_, new_state))) = timeout(Duration::from_millis(150), response_receiver.recv()).await {
            board_state = new_state;
        }
        // Drain any additional pending state updates so UI reflects latest comments/members
        while let Ok((_, new_state)) = response_receiver.try_recv() { board_state = new_state; }

        println!("=== 💬 Comment Board ===");
        println!("Comments: {} | Members: {}", board_state.comments.len(), board_state.room_members.len());
        for comment in &board_state.comments {
            println!("[{}] {}: {}", comment.timestamp, &comment.author[..8], comment.text);
        }
        println!("========================");

        input.clear();
        println!("Enter your comment (or 'quit', 'balance', 'unlock', 'bonds'):");
        std::io::stdin().read_line(&mut input).unwrap();
        let comment_text = input.trim();

        if comment_text == "quit" {
            exit_signal.store(true, Ordering::Relaxed);
            break;
        }

        if ["balance", "unlock", "bonds"].contains(&comment_text) {
            commands::handle_command(comment_text, &mut init_state.utxo_manager).await;
            continue;
        }

        if comment_text.is_empty() {
            println!("Comment cannot be empty!");
            continue;
        }

        // Submit comment logic
        // Determine bond amount (sompis): CLI override > organizer min-bond > room rules > 0
        let bond_amount = if args.bonds {
            if let Some(kas) = args.bond_amount {
                (kas * 100_000_000.0).round() as u64
            } else if let Some(min_kas) = args.min_bond {
                (min_kas * 100_000_000.0).round() as u64
            } else {
                // Use room's min_bond as default if available
                board_state.room_rules.min_bond
            }
        } else {
            0
        };

        if args.bonds && args.bond_amount.is_none() {
            let display_bond_kas = bond_amount as f64 / 100_000_000.0;
            println!("Required bond: {:.6} KAS (override with --bond-amount)", display_bond_kas);
        }
        let cmd = ContractCommand::SubmitComment { text: comment_text.to_string(), bond_amount, bond_output_index: Some(0), bond_script: None };
        let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(init_state.episode_id, cmd, participant_sk, participant_pk);

        // Capture pre-submit counts to detect the next state
        let prev_count = board_state.total_comments;

        if bond_amount == 0 {
            // No bond: submit a simple payload transaction without creating a 0-value output
            let tx = init_state
                .generator
                .build_command_transaction(init_state.utxo.clone(), &kaspa_addr, &step, crate::utils::FEE);
            match crate::utils::submit_tx_retry(&kaspad, tx.as_ref(), 3).await {
                Ok(()) => {
                    init_state.utxo = kdapp::generator::get_first_output_utxo(&tx);
                    println!("✅ Comment submitted successfully! TxID: {}", tx.id());
                    let _ = init_state.utxo_manager.refresh_utxos().await;
                    // Wait for at least one state update (reorgs may push multiple)
                    wait_for_any_state(&mut board_state, &mut response_receiver, 2000).await;
                }
                Err(e) => {
                    println!("❌ Failed to submit comment: {}", e);
                }
            }
        } else {
            // Bonded comment: create combined bond+payload transaction
            match init_state
                .utxo_manager
                .submit_comment_with_bond_payload(&step, bond_amount, 600, PATTERN, PREFIX, args.script_bonds)
                .await
            {
                Ok(txid) => {
                    println!("✅ Comment submitted successfully! TxID: {}", txid);
                    let _ = init_state.utxo_manager.refresh_utxos().await;
                    // Wait for at least one state update (reorgs may push multiple)
                    wait_for_any_state(&mut board_state, &mut response_receiver, 2000).await;
                }
                Err(e) => {
                    println!("❌ Failed to submit comment: {}", e);
                }
            }
        }

        // Drain any additional updates that might have arrived
        while let Ok((_, new_state)) = response_receiver.try_recv() { board_state = new_state; }
    }
}

async fn wait_for_any_state(
    board_state: &mut ContractState,
    response_receiver: &mut UnboundedReceiver<(EpisodeId, ContractState)>,
    timeout_ms: u64,
) {
    if let Ok(Some((_, new_state))) = timeout(Duration::from_millis(timeout_ms), response_receiver.recv()).await {
        *board_state = new_state;
    }
    // Drain anything else queued immediately after
    while let Ok((_, new_state)) = response_receiver.try_recv() { *board_state = new_state; }
}
