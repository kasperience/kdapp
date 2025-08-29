use clap::{Parser, Subcommand};
use kdapp::engine::{Engine, EngineMsg, EpisodeMessage};
use kdapp::generator::{Payload, PatternType, PrefixType, TransactionGenerator};
use kaspa_addresses::{Address, Prefix as AddrPrefix, Version as AddrVersion};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_wrpc_client::prelude::*;
use log::info;
use secp256k1::{rand::rngs::OsRng, Keypair, SecretKey};
use std::sync::{atomic::AtomicBool, mpsc, Arc};
use std::thread;
use tokio::signal;

mod episode;
mod handler;
mod tlv;
mod watchtower;

use episode::{LotteryCommand, LotteryEpisode};

const PREFIX: PrefixType = u32::from_le_bytes(*b"KDRW");
const PATTERN: PatternType = [
    (0, 1), (1, 0), (2, 1), (3, 0),
    (4, 1), (5, 0), (6, 1), (7, 0),
    (8, 1), (9, 0),
];

#[derive(Parser, Debug)]
#[command(name = "kas-draw", version, about = "Kaspa lottery episode (M1 MVP)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    New { #[arg(long)] episode_id: u32 },
    Buy {
        #[arg(long)] episode_id: u32,
        #[arg(long)] amount: u64,
        #[arg(value_name = "N", num_args = 5)] numbers: Vec<u8>,
    },
    Draw { #[arg(long)] episode_id: u32, #[arg(long)] entropy: String },
    Claim { #[arg(long)] episode_id: u32, #[arg(long)] ticket_id: u64, #[arg(long)] round: u64 },
    /// Start engine + proxy listener (L1 mode). Stop with Ctrl+C.
    Engine { #[arg(long)] mainnet: bool, #[arg(long)] wrpc_url: Option<String> },
    /// Submit a NewEpisode transaction carrying participants (your pubkey)
    SubmitNew {
        #[arg(long)] episode_id: u32,
        #[arg(long)] kaspa_private_key: String,
        #[arg(long)] mainnet: bool,
        #[arg(long)] wrpc_url: Option<String>,
    },
    /// Submit a BuyTicket transaction
    SubmitBuy {
        #[arg(long)] episode_id: u32,
        #[arg(long)] kaspa_private_key: String,
        #[arg(long)] mainnet: bool,
        #[arg(long)] wrpc_url: Option<String>,
        #[arg(long)] amount: u64,
        #[arg(value_name = "N", num_args = 5)] numbers: Vec<u8>,
    },
    /// Submit a Draw transaction
    SubmitDraw {
        #[arg(long)] episode_id: u32,
        #[arg(long)] kaspa_private_key: String,
        #[arg(long)] mainnet: bool,
        #[arg(long)] wrpc_url: Option<String>,
        #[arg(long)] entropy: String,
    },
    /// Submit a Claim transaction
    SubmitClaim {
        #[arg(long)] episode_id: u32,
        #[arg(long)] kaspa_private_key: String,
        #[arg(long)] mainnet: bool,
        #[arg(long)] wrpc_url: Option<String>,
        #[arg(long)] ticket_id: u64,
        #[arg(long)] round: u64,
    },
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::New { episode_id } => {
            // Build NewEpisode payload and print id/prefix/pattern for logs
            let cmd = EpisodeMessage::<LotteryEpisode>::NewEpisode { episode_id, participants: vec![] };
            let payload = borsh::to_vec(&cmd).unwrap();
            let packed = Payload::pack_header(payload, PREFIX);
            info!("kas-draw NEW episode {} payload {} bytes (prefix=KDRW)", episode_id, packed.len());
            // In real run, submit tx via generator; here we just log size
        }
        Commands::Buy { episode_id, amount, numbers } => {
            let nums = [numbers[0], numbers[1], numbers[2], numbers[3], numbers[4]];
            let cmd = LotteryCommand::BuyTicket { numbers: nums, entry_amount: amount };
            // In practice: sign and wrap as EpisodeMessage::SignedCommand
            let sk = SecretKey::new(&mut OsRng);
            let pk = kdapp::pki::PubKey(secp256k1::PublicKey::from_secret_key(&secp256k1::SECP256K1, &sk));
            let msg = EpisodeMessage::<LotteryEpisode>::new_signed_command(episode_id, cmd, sk, pk);
            let payload = borsh::to_vec(&msg).unwrap();
            let packed = Payload::pack_header(payload, PREFIX);
            info!("kas-draw BUY ticket ep {} ({} bytes)", episode_id, packed.len());
        }
        Commands::Draw { episode_id, entropy } => {
            let cmd = LotteryCommand::ExecuteDraw { entropy_source: entropy };
            let msg = EpisodeMessage::<LotteryEpisode>::UnsignedCommand { episode_id, cmd };
            let payload = borsh::to_vec(&msg).unwrap();
            let packed = Payload::pack_header(payload, PREFIX);
            info!("kas-draw DRAW ep {} ({} bytes)", episode_id, packed.len());
        }
        Commands::Claim { episode_id, ticket_id, round } => {
            let cmd = LotteryCommand::ClaimPrize { ticket_id, round };
            let msg = EpisodeMessage::<LotteryEpisode>::UnsignedCommand { episode_id, cmd };
            let payload = borsh::to_vec(&msg).unwrap();
            let packed = Payload::pack_header(payload, PREFIX);
            info!("kas-draw CLAIM ep {} ({} bytes)", episode_id, packed.len());
        }
        Commands::Engine { mainnet, wrpc_url } => {
            let (sender, receiver) = mpsc::channel::<EngineMsg>();
            let mut engine: Engine<LotteryEpisode, crate::handler::Handler> = Engine::new(receiver);
            let engine_thread = thread::spawn(move || {
                engine.start(vec![crate::handler::Handler]);
            });

            let network = if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
            let kaspad = kdapp::proxy::connect_client(network, wrpc_url).await.expect("kaspad connect");
            let exit = Arc::new(AtomicBool::new(false));
            let exit2 = exit.clone();
            let listener = tokio::spawn(async move {
                kdapp::proxy::run_listener(kaspad, std::iter::once((PREFIX, (PATTERN, sender))).collect(), exit2).await;
            });
            info!("engine running; press Ctrl+C to exit");
            let _ = signal::ctrl_c().await;
            exit.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = listener.await;
            let _ = engine_thread.join();
        }
        Commands::SubmitNew { episode_id, kaspa_private_key, mainnet, wrpc_url } => {
            submit_tx_flow(SubmitKind::New { episode_id }, &kaspa_private_key, mainnet, wrpc_url).await;
        }
        Commands::SubmitBuy { episode_id, kaspa_private_key, mainnet, wrpc_url, amount, numbers } => {
            let nums = [numbers[0], numbers[1], numbers[2], numbers[3], numbers[4]];
            submit_tx_flow(SubmitKind::Buy { episode_id, amount, numbers: nums }, &kaspa_private_key, mainnet, wrpc_url).await;
        }
        Commands::SubmitDraw { episode_id, kaspa_private_key, mainnet, wrpc_url, entropy } => {
            submit_tx_flow(SubmitKind::Draw { episode_id, entropy }, &kaspa_private_key, mainnet, wrpc_url).await;
        }
        Commands::SubmitClaim { episode_id, kaspa_private_key, mainnet, wrpc_url, ticket_id, round } => {
            submit_tx_flow(SubmitKind::Claim { episode_id, ticket_id, round }, &kaspa_private_key, mainnet, wrpc_url).await;
        }
    }
}

const FEE: u64 = 5_000;

enum SubmitKind {
    New { episode_id: u32 },
    Buy { episode_id: u32, amount: u64, numbers: [u8; 5] },
    Draw { episode_id: u32, entropy: String },
    Claim { episode_id: u32, ticket_id: u64, round: u64 },
}

async fn submit_tx_flow(kind: SubmitKind, sk_hex: &str, mainnet: bool, wrpc_url: Option<String>) {
    // Build signer + address
    let mut private_key_bytes = [0u8; 32];
    if faster_hex::hex_decode(sk_hex.as_bytes(), &mut private_key_bytes).is_err() {
        eprintln!("invalid --kaspa-private-key hex");
        return;
    }
    let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).expect("invalid sk");
    let network = if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
    let addr_prefix = if mainnet { AddrPrefix::Mainnet } else { AddrPrefix::Testnet };
    let addr = Address::new(addr_prefix, AddrVersion::PubKey, &keypair.x_only_public_key().0.serialize());
    info!("funding address: {}", addr);

    // Connect and fetch UTXOs
    let kaspad = kdapp::proxy::connect_client(network, wrpc_url).await.expect("kaspad connect");
    let utxos = kaspad
        .get_utxos_by_addresses(vec![addr.clone()])
        .await
        .expect("get utxos")
        .into_iter()
        .map(|u| (kaspa_consensus_core::tx::TransactionOutpoint::from(u.outpoint), kaspa_consensus_core::tx::UtxoEntry::from(u.utxo_entry)))
        .collect::<Vec<_>>();
    if utxos.is_empty() {
        eprintln!("no UTXOs for {}", addr);
        return;
    }
    // Pick the largest UTXO
    let (op, entry) = utxos
        .iter()
        .max_by_key(|(_, e)| e.amount)
        .expect("has utxo")
        .clone();
    if entry.amount <= FEE {
        eprintln!("selected UTXO too small: {}", entry.amount);
        return;
    }

    // Build command payload
    let episode_id = match &kind { SubmitKind::New { episode_id } => *episode_id, SubmitKind::Buy { episode_id, .. } => *episode_id, SubmitKind::Draw { episode_id, .. } => *episode_id, SubmitKind::Claim { episode_id, .. } => *episode_id };
    let pk = kdapp::pki::PubKey(keypair.public_key());
    let msg = match kind {
        SubmitKind::New { episode_id } => EpisodeMessage::<crate::episode::LotteryEpisode>::NewEpisode { episode_id, participants: vec![pk] },
        SubmitKind::Buy { episode_id, amount, numbers } => {
            let cmd = crate::episode::LotteryCommand::BuyTicket { numbers, entry_amount: amount };
            EpisodeMessage::<crate::episode::LotteryEpisode>::new_signed_command(episode_id, cmd, keypair.secret_key(), pk)
        }
        SubmitKind::Draw { episode_id, entropy } => {
            let cmd = crate::episode::LotteryCommand::ExecuteDraw { entropy_source: entropy };
            EpisodeMessage::<crate::episode::LotteryEpisode>::UnsignedCommand { episode_id, cmd }
        }
        SubmitKind::Claim { episode_id, ticket_id, round } => {
            let cmd = crate::episode::LotteryCommand::ClaimPrize { ticket_id, round };
            EpisodeMessage::<crate::episode::LotteryEpisode>::UnsignedCommand { episode_id, cmd }
        }
    };

    // Build and submit transaction carrying the payload
    let gen = TransactionGenerator::new(keypair, PATTERN, PREFIX);
    let tx = gen.build_command_transaction((op, entry), &addr, &msg, FEE);
    info!("built tx {} (payload)", tx.id());
    if let Err(e) = submit_tx_retry(&kaspad, &tx, 3).await {
        eprintln!("submit failed: {}", e);
    } else {
        info!("submitted {}", tx.id());
    }
}

async fn submit_tx_retry(kaspad: &KaspaRpcClient, tx: &kaspa_consensus_core::tx::Transaction, attempts: usize) -> Result<(), String> {
    let mut tries = 0usize;
    loop {
        match kaspad.submit_transaction(tx.into(), false).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                tries += 1;
                let msg = e.to_string();
                if tries >= attempts { return Err(format!("submit failed after {} attempts: {}", tries, msg)); }
                if msg.contains("WebSocket") || msg.contains("not connected") || msg.contains("disconnected") {
                    let _ = kaspad.connect(Some(kdapp::proxy::connect_options())).await;
                    continue;
                } else if msg.contains("orphan") { continue; }
                else if msg.contains("already accepted") { return Ok(()); }
                else { return Err(format!("submit failed: {}", msg)); }
            }
        }
    }
}
