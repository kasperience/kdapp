use clap::Parser;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kdapp::engine::{Engine, EngineMsg, EpisodeMessage};
use kdapp::generator::Payload;
use log::info;
use secp256k1::{rand::rngs::OsRng, Keypair, SecretKey};
use std::sync::{atomic::AtomicBool, mpsc, Arc};
use std::thread;
use tokio::signal;

mod episode;
mod handler;
mod offchain;
mod tlv;
mod watchtower;
mod routing;
mod cli;
mod submit;
mod offchain_client;

use cli::{Cli, Commands};
use episode::{LotteryCommand, LotteryEpisode};
use offchain_client::{auto_seq, send_tlv_cmd, send_tlv_close, send_tlv_new, send_with_ack};
use submit::{get_wrpc_url, resolve_dev_key_hex, submit_checkpoint_tx, submit_tx_flow, SubmitKind};

#[tokio::main]
async fn main() {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::New { episode_id } => {
            let cmd = EpisodeMessage::<LotteryEpisode>::NewEpisode { episode_id, participants: vec![] };
            let payload = borsh::to_vec(&cmd).unwrap();
            let packed = Payload::pack_header(payload, routing::PREFIX);
            info!("kas-draw NEW episode {} payload {} bytes (prefix=KDRW)", episode_id, packed.len());
        }
        Commands::Buy { episode_id, amount, numbers } => {
            let nums = [numbers[0], numbers[1], numbers[2], numbers[3], numbers[4]];
            let cmd = LotteryCommand::BuyTicket { numbers: nums, entry_amount: amount };
            let sk = SecretKey::new(&mut OsRng);
            let pk = kdapp::pki::PubKey(secp256k1::PublicKey::from_secret_key(secp256k1::SECP256K1, &sk));
            let msg = EpisodeMessage::<LotteryEpisode>::new_signed_command(episode_id, cmd, sk, pk);
            let payload = borsh::to_vec(&msg).unwrap();
            let packed = Payload::pack_header(payload, routing::PREFIX);
            info!("kas-draw BUY ticket ep {} ({} bytes)", episode_id, packed.len());
        }
        Commands::Draw { episode_id, entropy } => {
            let cmd = LotteryCommand::ExecuteDraw { entropy_source: entropy };
            let msg = EpisodeMessage::<LotteryEpisode>::UnsignedCommand { episode_id, cmd };
            let payload = borsh::to_vec(&msg).unwrap();
            let packed = Payload::pack_header(payload, routing::PREFIX);
            info!("kas-draw DRAW ep {} ({} bytes)", episode_id, packed.len());
        }
        Commands::Claim { episode_id, ticket_id, round } => {
            let cmd = LotteryCommand::ClaimPrize { ticket_id, round };
            let msg = EpisodeMessage::<LotteryEpisode>::UnsignedCommand { episode_id, cmd };
            let payload = borsh::to_vec(&msg).unwrap();
            let packed = Payload::pack_header(payload, routing::PREFIX);
            info!("kas-draw CLAIM ep {} ({} bytes)", episode_id, packed.len());
        }
        Commands::Engine { mainnet, wrpc_url } => {
            let (sender, receiver) = mpsc::channel::<EngineMsg>();
            let mut engine: Engine<LotteryEpisode, crate::handler::Handler> = Engine::new(receiver);
            let engine_thread = thread::spawn(move || {
                engine.start(vec![crate::handler::Handler::new()]);
            });

            let network =
                if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
            let url_opt = get_wrpc_url(wrpc_url);
            let kaspad = kdapp::proxy::connect_client(network, url_opt).await.expect("kaspad connect");
            let exit = Arc::new(AtomicBool::new(false));
            let exit2 = exit.clone();
            let listener = tokio::spawn(async move {
                kdapp::proxy::run_listener(
                    kaspad,
                    std::iter::once((routing::PREFIX, (routing::pattern(), sender))).collect(),
                    exit2,
                )
                .await;
            });
            info!("engine running; press Ctrl+C to exit");
            let _ = signal::ctrl_c().await;
            exit.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = listener.await;
            let _ = engine_thread.join();
        }
        Commands::SubmitNew { episode_id, kaspa_private_key, mainnet, wrpc_url } => {
            submit_tx_flow(SubmitKind::New { episode_id }, kaspa_private_key, mainnet, wrpc_url).await;
        }
        Commands::SubmitBuy { episode_id, kaspa_private_key, mainnet, wrpc_url, amount, numbers } => {
            let nums = [numbers[0], numbers[1], numbers[2], numbers[3], numbers[4]];
            submit_tx_flow(
                SubmitKind::Buy { episode_id, amount, numbers: nums },
                kaspa_private_key,
                mainnet,
                wrpc_url,
            )
            .await;
        }
        Commands::SubmitDraw { episode_id, kaspa_private_key, mainnet, wrpc_url, entropy } => {
            submit_tx_flow(SubmitKind::Draw { episode_id, entropy }, kaspa_private_key, mainnet, wrpc_url).await;
        }
        Commands::SubmitClaim { episode_id, kaspa_private_key, mainnet, wrpc_url, ticket_id, round } => {
            submit_tx_flow(
                SubmitKind::Claim { episode_id, ticket_id, round },
                kaspa_private_key,
                mainnet,
                wrpc_url,
            )
            .await;
        }
        Commands::SubmitCheckpoint { episode_id, seq, state_root, kaspa_private_key, mainnet, wrpc_url } => {
            let mut root = [0u8; 32];
            if hex::decode_to_slice(state_root.as_bytes(), &mut root).is_err() {
                eprintln!("invalid --state-root hex");
                return;
            }
            if let Err(e) = submit_checkpoint_tx(episode_id as u64, seq, root, kaspa_private_key, mainnet, wrpc_url).await {
                eprintln!("submit-checkpoint failed: {e}");
            }
        }
        Commands::OffchainEngine { bind, no_ack, no_close } => {
            use std::sync::mpsc;
            let (sender, receiver) = mpsc::channel::<EngineMsg>();
            let mut engine: Engine<LotteryEpisode, crate::handler::Handler> = Engine::new(receiver);
            let handler = crate::handler::Handler::with_tower(crate::watchtower::SimTower::new());
            let engine_thread = thread::spawn(move || {
                engine.start(vec![handler]);
            });

            let router = crate::offchain::OffchainRouter::new(sender, !no_ack, !no_close);
            router.run_udp(&bind);
            let _ = engine_thread.join();
        }
        Commands::OffchainSend {
            r#type,
            episode_id,
            router,
            force_seq,
            no_ack,
            kaspa_private_key,
            amount,
            numbers,
            entropy,
            ticket_id,
            round,
            state_root,
        } => {
            if r#type != "new" && r#type != "cmd" && r#type != "close" && r#type != "ckpt" {
                eprintln!("--type must be one of: new|cmd|close|ckpt");
                return;
            }
            let dest = router.unwrap_or_else(|| "127.0.0.1:18181".to_string());
            let seq = match force_seq {
                Some(s) => s,
                None => auto_seq(episode_id as u64, &r#type),
            };
            if r#type == "ckpt" {
                let Some(root_hex) = state_root else {
                    eprintln!("--state-root <hex32> is required for ckpt");
                    return;
                };
                let mut root = [0u8; 32];
                if hex::decode_to_slice(root_hex.as_bytes(), &mut root).is_err() {
                    eprintln!("invalid --state-root hex");
                    return;
                }
                let tlv = crate::tlv::TlvMsg {
                    version: crate::tlv::TLV_VERSION,
                    msg_type: crate::tlv::MsgType::Checkpoint as u8,
                    episode_id: episode_id as u64,
                    seq,
                    state_hash: root,
                    payload: vec![],
                };
                send_with_ack(&dest, tlv, false, !no_ack);
                return;
            }
            if r#type == "close" {
                send_tlv_close(&dest, episode_id as u64, seq, !no_ack);
                return;
            }
            if r#type == "new" {
                let participants = if let Some(sk_hex) = resolve_dev_key_hex(&kaspa_private_key) {
                    let mut private_key_bytes = [0u8; 32];
                    if faster_hex::hex_decode(sk_hex.trim().as_bytes(), &mut private_key_bytes).is_err() {
                        eprintln!("invalid private key hex (dev key/env/flag)");
                        return;
                    }
                    let keypair =
                        Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).expect("invalid sk");
                    vec![kdapp::pki::PubKey(keypair.public_key())]
                } else {
                    vec![]
                };
                let cmd = EpisodeMessage::<LotteryEpisode>::NewEpisode { episode_id, participants };
                send_tlv_new(&dest, episode_id as u64, seq, cmd, !no_ack);
                return;
            }
            let emsg = if let (Some(a), ns) = (amount, &numbers) {
                if ns.len() != 5 {
                    eprintln!("provide exactly 5 numbers for --amount mode");
                    return;
                }
                let nums = [ns[0], ns[1], ns[2], ns[3], ns[4]];
                let cmd = LotteryCommand::BuyTicket { numbers: nums, entry_amount: a };
                if let Some(sk_hex) = resolve_dev_key_hex(&kaspa_private_key) {
                    let mut private_key_bytes = [0u8; 32];
                    if faster_hex::hex_decode(sk_hex.trim().as_bytes(), &mut private_key_bytes).is_err() {
                        eprintln!("invalid private key hex (dev key/env/flag)");
                        return;
                    }
                    let keypair =
                        Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).expect("invalid sk");
                    let pk = kdapp::pki::PubKey(keypair.public_key());
                    EpisodeMessage::<LotteryEpisode>::new_signed_command(episode_id, cmd, keypair.secret_key(), pk)
                } else {
                    EpisodeMessage::<LotteryEpisode>::UnsignedCommand { episode_id, cmd }
                }
            } else if let Some(ent) = entropy {
                let cmd = LotteryCommand::ExecuteDraw { entropy_source: ent };
                EpisodeMessage::<LotteryEpisode>::UnsignedCommand { episode_id, cmd }
            } else if let (Some(tid), Some(r)) = (ticket_id, round) {
                let cmd = LotteryCommand::ClaimPrize { ticket_id: tid, round: r };
                EpisodeMessage::<LotteryEpisode>::UnsignedCommand { episode_id, cmd }
            } else {
                eprintln!("specify one of: --amount <u64> <5 nums> | --entropy <str> | --ticket-id <u64> --round <u64>");
                return;
            };
            send_tlv_cmd(&dest, episode_id as u64, seq, emsg, !no_ack);
        }
    }
}

