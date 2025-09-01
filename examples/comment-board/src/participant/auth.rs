use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_wrpc_client::prelude::*;
use kdapp::{engine::EpisodeMessage, episode::EpisodeId, generator, pki::PubKey};
use log::*;
use secp256k1::{Message, SecretKey};
use std::sync::{atomic::AtomicBool, Arc};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    episode::board_with_contract::{ContractCommentBoard, ContractState},
    episode::commands::ContractCommand,
    utils::FEE,
};

pub async fn perform_authentication(
    kaspad: &KaspaRpcClient,
    generator: &generator::TransactionGenerator,
    mut state: ContractState,
    mut response_receiver: UnboundedReceiver<(EpisodeId, ContractState)>,
    _exit_signal: &Arc<AtomicBool>,
    participant_sk: SecretKey,
    participant_pk: PubKey,
    episode_id: EpisodeId,
    utxo: &mut (TransactionOutpoint, UtxoEntry),
    kaspa_addr: &Address,
) -> Result<(ContractState, UnboundedReceiver<(EpisodeId, ContractState)>), Box<dyn std::error::Error>> {
    if !state.authenticated_users.contains(&format!("{participant_pk}")) {
        println!("🔑 Requesting authentication challenge...");
        let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(
            episode_id,
            ContractCommand::RequestChallenge,
            participant_sk,
            participant_pk,
        );

        let tx = generator.build_command_transaction(utxo.clone(), kaspa_addr, &step, FEE);
        info!("💰 Submitting RequestChallenge (you pay): {}", tx.id());
        crate::utils::submit_tx_retry(kaspad, tx.as_ref(), 3).await.map_err(|e| e.to_string())?;
        *utxo = generator::get_first_output_utxo(&tx);

        let challenge_text: String = loop {
            if let Some((received_id, new_state)) = response_receiver.recv().await {
                if received_id == episode_id {
                    if let Some(c) = &new_state.current_challenge {
                        println!("✅ Received challenge: {c}");
                        break c.clone();
                    }
                }
            } else {
                println!("❌ Failed to receive challenge: Channel closed");
                return Err("Channel closed".into());
            }
        };

        if !challenge_text.is_empty() {
            println!("✍️ Signing challenge and submitting response...");
            use sha2::{Digest, Sha256};
            let secp = secp256k1::Secp256k1::new();
            let mut hasher = Sha256::new();
            hasher.update(challenge_text.as_bytes());
            let message = Message::from_digest(hasher.finalize().into());
            let signature = secp.sign_ecdsa(&message, &participant_sk);
            let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(
                episode_id,
                ContractCommand::SubmitResponse { signature: signature.to_string(), nonce: challenge_text },
                participant_sk,
                participant_pk,
            );

            let tx = generator.build_command_transaction(utxo.clone(), kaspa_addr, &step, FEE);
            info!("💰 Submitting SubmitResponse (you pay): {}", tx.id());
            crate::utils::submit_tx_retry(kaspad, tx.as_ref(), 3).await.map_err(|e| e.to_string())?;
            *utxo = generator::get_first_output_utxo(&tx);

            loop {
                if let Some((received_id, new_state)) = response_receiver.recv().await {
                    if received_id == episode_id && new_state.authenticated_users.contains(&format!("{participant_pk}")) {
                        println!("✅ Successfully authenticated!");
                        state = new_state;
                        break;
                    }
                } else {
                    println!("❌ Failed to receive authentication confirmation: Channel closed");
                    return Err("Channel closed".into());
                }
            }
        }
    } else {
        println!("🎯 Already authenticated!");
    }

    Ok((state, response_receiver))
}
