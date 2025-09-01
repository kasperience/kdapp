pub mod auth;
pub mod commands;
pub mod init;
pub mod main_loop;

use crate::{
    cli::Args,
    episode::{board_with_contract::ContractCommentBoard, handler::CommentHandler},
    utils::{PATTERN, PREFIX},
};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kdapp::{
    engine,
    pki::PubKey,
    proxy::{self, connect_client},
};

use log::*;
use secp256k1::Keypair;
use std::sync::{atomic::AtomicBool, mpsc::channel, Arc};

pub async fn run_participant(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let (network, prefix) = if args.mainnet {
        (NetworkId::new(NetworkType::Mainnet), Prefix::Mainnet)
    } else {
        (NetworkId::with_suffix(NetworkType::Testnet, 10), Prefix::Testnet)
    };

    let kaspa_signer = if let Some(ref private_key_hex) = args.kaspa_private_key {
        let mut private_key_bytes = [0u8; 32];
        faster_hex::hex_decode(private_key_hex.as_bytes(), &mut private_key_bytes)?;
        Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes)?
    } else {
        let (sk, pk) = &secp256k1::generate_keypair(&mut rand::thread_rng());
        info!(
            "Generated private key {} and address {}. Send some funds to this address and rerun with `--kaspa-private-key {}`",
            sk.display_secret(),
            String::from(&Address::new(prefix, Version::PubKey, &pk.x_only_public_key().0.serialize())),
            sk.display_secret()
        );
        return Ok(());
    };

    let kaspa_addr = Address::new(prefix, Version::PubKey, &kaspa_signer.x_only_public_key().0.serialize());
    let participant_pk = PubKey(kaspa_signer.public_key());
    let participant_sk = kaspa_signer.secret_key();

    info!("Your identity (public key): {participant_pk}");
    info!("Your Kaspa address: {kaspa_addr}");

    let kaspad = connect_client(network, args.wrpc_url.clone()).await?;
    let participant_kaspad = connect_client(network, args.wrpc_url.clone()).await?;

    let (sender, receiver) = channel();
    let (response_sender, response_receiver) = tokio::sync::mpsc::unbounded_channel();
    let exit_signal = Arc::new(AtomicBool::new(false));
    let exit_signal_receiver = exit_signal.clone();

    let mut engine = engine::Engine::<ContractCommentBoard, CommentHandler>::new(receiver);
    let engine_task = tokio::task::spawn_blocking(move || {
        engine.start(vec![CommentHandler { sender: response_sender, _participant: participant_pk }]);
    });

    let args_clone = args.clone();
    let participant_task = tokio::spawn(async move {
        main_loop::run_comment_board(
            participant_kaspad,
            kaspa_signer,
            kaspa_addr,
            response_receiver,
            exit_signal,
            participant_sk,
            participant_pk,
            args_clone.room_episode_id,
            args_clone,
        )
        .await;
    });

    proxy::run_listener(kaspad, std::iter::once((PREFIX, (PATTERN, sender))).collect(), exit_signal_receiver).await;

    engine_task.await?;
    participant_task.await?;

    Ok(())
}
