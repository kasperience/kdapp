use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::select;
use tokio::sync::mpsc::UnboundedReceiver;

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
use kaspa_wrpc_client::prelude::KaspaRpcClient;

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
    let (mut init_state, mut response_receiver) =
        match init::initialize_participant(&kaspad, kaspa_signer, kaspa_addr.clone(), response_receiver, target_episode_id).await {
            Ok(state) => state,
            Err(e) => {
                println!("❌ Initialization failed: {e}");
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
    )
    .await
    {
        Ok(state) => state,
        Err(e) => {
            println!("❌ Authentication failed: {e}");
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
        let tx = init_state.generator.build_command_transaction(init_state.utxo.clone(), &kaspa_addr, &join_step, crate::utils::FEE);
        let _ = crate::utils::submit_tx_retry(&kaspad, tx.as_ref(), 3).await;
        init_state.utxo = kdapp::generator::get_first_output_utxo(&tx);
    }

    // 4. Main Loop (non-blocking input using std thread + channel)
    fn render(state: &ContractState) {
        println!("=== 💬 Comment Board ===");
        println!("Comments: {} | Members: {}", state.comments.len(), state.room_members.len());
        for comment in &state.comments {
            println!("[{}] {}: {}", comment.timestamp, &comment.author[..8], comment.text);
        }
        println!("========================");
        println!("Enter your comment (or 'quit', 'balance', 'unlock', 'bonds'):");
    }

    let (in_tx, mut in_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut line = String::new();
        loop {
            line.clear();
            if stdin.read_line(&mut line).is_err() {
                break;
            }
            let s = line.trim().to_string();
            if in_tx.send(s).is_err() {
                break;
            }
        }
    });

    render(&board_state);
    loop {
        select! {
            maybe_state = response_receiver.recv() => {
                if let Some((_, new_state)) = maybe_state {
                    board_state = new_state;
                    render(&board_state);
                } else {
                    println!("❌ Engine channel closed");
                    break;
                }
            }
            maybe_line = in_rx.recv() => {
                match maybe_line {
                    Some(line) => {
                        let comment_text = line.trim();

                        if comment_text == "quit" { exit_signal.store(true, Ordering::Relaxed); break; }

                        if ["balance", "unlock", "bonds"].contains(&comment_text) {
                            commands::handle_command(comment_text, &mut init_state.utxo_manager).await;
                            render(&board_state);
                            continue;
                        }

                        if comment_text.is_empty() {
                            println!("Comment cannot be empty!");
                            render(&board_state);
                            continue;
                        }

                        let bond_amount = if args.bonds {
                            if let Some(kas) = args.bond_amount { (kas * 100_000_000.0).round() as u64 }
                            else if let Some(min_kas) = args.min_bond { (min_kas * 100_000_000.0).round() as u64 }
                            else { board_state.room_rules.min_bond }
                        } else { 0 };

                        if args.bonds && args.bond_amount.is_none() {
                            let display_bond_kas = bond_amount as f64 / 100_000_000.0;
                            println!("Required bond: {display_bond_kas:.6} KAS (override with --bond-amount)");
                        }

                        let cmd = ContractCommand::SubmitComment { text: comment_text.to_string(), bond_amount, bond_output_index: Some(0), bond_script: None };
                        let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(init_state.episode_id, cmd, participant_sk, participant_pk);

                        if bond_amount == 0 {
                            let tx = init_state
                                .generator
                                .build_command_transaction(init_state.utxo.clone(), &kaspa_addr, &step, crate::utils::FEE);
                            match crate::utils::submit_tx_retry(&kaspad, tx.as_ref(), 3).await {
                                Ok(()) => {
                                    init_state.utxo = kdapp::generator::get_first_output_utxo(&tx);
                                    println!("✅ Comment submitted successfully! TxID: {}", tx.id());
                                    let _ = init_state.utxo_manager.refresh_utxos().await;
                                }
                                Err(e) => println!("❌ Failed to submit comment: {e}"),
                            }
                        } else {
                            match init_state
                                .utxo_manager
                                .submit_comment_with_bond_payload(&step, bond_amount, 600, PATTERN, PREFIX, args.script_bonds)
                                .await
                            {
                                Ok(txid) => {
                                    println!("✅ Comment submitted successfully! TxID: {txid}");
                                    let _ = init_state.utxo_manager.refresh_utxos().await;
                                }
                                Err(e) => println!("❌ Failed to submit comment: {e}"),
                            }
                        }

                        render(&board_state);
                    }
                    None => { break; }
                }
            }
        }
    }
}
