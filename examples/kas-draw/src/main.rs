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
mod offchain;
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
    /// Submit a Checkpoint payload to L1 (OKCP v1)
    SubmitCheckpoint {
        #[arg(long)] episode_id: u32,
        #[arg(long)] seq: u64,
        #[arg(long)] state_root: String,
        #[arg(long)] kaspa_private_key: Option<String>,
        #[arg(long)] mainnet: bool,
        #[arg(long)] wrpc_url: Option<String>,
    },
    /// Start engine + proxy listener (L1 mode). Stop with Ctrl+C.
    Engine { #[arg(long)] mainnet: bool, #[arg(long)] wrpc_url: Option<String> },
    /// Start off-chain engine + in-proc UDP router. Stop with Ctrl+C.
    OffchainEngine {
        #[arg(long, default_value_t = String::from("127.0.0.1:18181"))] bind: String,
        #[arg(long)] no_ack: bool,
        #[arg(long)] no_close: bool,
    },
    /// Submit a NewEpisode transaction carrying participants (your pubkey)
    SubmitNew {
        #[arg(long)] episode_id: u32,
        #[arg(long)] kaspa_private_key: Option<String>,
        #[arg(long)] mainnet: bool,
        #[arg(long)] wrpc_url: Option<String>,
    },
    /// Submit a BuyTicket transaction
    SubmitBuy {
        #[arg(long)] episode_id: u32,
        #[arg(long)] kaspa_private_key: Option<String>,
        #[arg(long)] mainnet: bool,
        #[arg(long)] wrpc_url: Option<String>,
        #[arg(long)] amount: u64,
        #[arg(value_name = "N", num_args = 5)] numbers: Vec<u8>,
    },
    /// Submit a Draw transaction
    SubmitDraw {
        #[arg(long)] episode_id: u32,
        #[arg(long)] kaspa_private_key: Option<String>,
        #[arg(long)] mainnet: bool,
        #[arg(long)] wrpc_url: Option<String>,
        #[arg(long)] entropy: String,
    },
    /// Submit a Claim transaction
    SubmitClaim {
        #[arg(long)] episode_id: u32,
        #[arg(long)] kaspa_private_key: Option<String>,
        #[arg(long)] mainnet: bool,
        #[arg(long)] wrpc_url: Option<String>,
        #[arg(long)] ticket_id: u64,
        #[arg(long)] round: u64,
    },
    /// Send a TLV v1 message to the off-chain router
    OffchainSend {
        #[arg(long)] r#type: String, // new|cmd|close|ckpt
        #[arg(long)] episode_id: u32,
        #[arg(long)] router: Option<String>,
        #[arg(long)] force_seq: Option<u64>,
        #[arg(long)] no_ack: bool,
        #[arg(long)] kaspa_private_key: Option<String>,
        // For Buy
        #[arg(long)] amount: Option<u64>,
        #[arg(value_name = "N", num_args = 5)] numbers: Vec<u8>,
        // For Draw
        #[arg(long)] entropy: Option<String>,
        // For Claim
        #[arg(long)] ticket_id: Option<u64>,
        #[arg(long)] round: Option<u64>,
        // For Checkpoint
        #[arg(long)] state_root: Option<String>,
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
                engine.start(vec![crate::handler::Handler::new()]);
            });

            let network = if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
            let url_opt = get_wrpc_url(wrpc_url);
            let kaspad = kdapp::proxy::connect_client(network, url_opt).await.expect("kaspad connect");
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
            submit_tx_flow(SubmitKind::New { episode_id }, kaspa_private_key, mainnet, wrpc_url).await;
        }
        Commands::SubmitBuy { episode_id, kaspa_private_key, mainnet, wrpc_url, amount, numbers } => {
            let nums = [numbers[0], numbers[1], numbers[2], numbers[3], numbers[4]];
            submit_tx_flow(SubmitKind::Buy { episode_id, amount, numbers: nums }, kaspa_private_key, mainnet, wrpc_url).await;
        }
        Commands::SubmitDraw { episode_id, kaspa_private_key, mainnet, wrpc_url, entropy } => {
            submit_tx_flow(SubmitKind::Draw { episode_id, entropy }, kaspa_private_key, mainnet, wrpc_url).await;
        }
        Commands::SubmitClaim { episode_id, kaspa_private_key, mainnet, wrpc_url, ticket_id, round } => {
            submit_tx_flow(SubmitKind::Claim { episode_id, ticket_id, round }, kaspa_private_key, mainnet, wrpc_url).await;
        }
        Commands::SubmitCheckpoint { episode_id, seq, state_root, kaspa_private_key, mainnet, wrpc_url } => {
            // Parse state_root
            let mut root = [0u8; 32];
            if let Err(_) = hex::decode_to_slice(state_root.as_bytes(), &mut root) { eprintln!("invalid --state-root hex"); return; }
            if let Err(e) = submit_checkpoint_tx(episode_id as u64, seq, root, kaspa_private_key, mainnet, wrpc_url).await {
                eprintln!("submit-checkpoint failed: {}", e);
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

            // Start router in this thread
            let router = crate::offchain::OffchainRouter::new(sender, !no_ack, !no_close);
            // Run until process is terminated (Ctrl+C)
            router.run_udp(&bind);
            let _ = engine_thread.join();
        }
        Commands::OffchainSend { r#type, episode_id, router, force_seq, no_ack, kaspa_private_key, amount, numbers, entropy, ticket_id, round } => {
            if r#type != "new" && r#type != "cmd" && r#type != "close" && r#type != "ckpt" {
                eprintln!("--type must be one of: new|cmd|close|ckpt");
                return;
            }
            let dest = router.unwrap_or_else(|| "127.0.0.1:18181".to_string());
            let seq = match force_seq { Some(s) => s, None => auto_seq(episode_id as u64, &r#type) };
            if r#type == "ckpt" {
                // Build a checkpoint TLV (payload empty). Require --state-root hex (32 bytes), attach into state_hash field.
                let Some(root_hex) = state_root else { eprintln!("--state-root <hex32> is required for ckpt"); return; };
                let mut root = [0u8; 32];
                if let Err(_) = hex::decode_to_slice(root_hex.as_bytes(), &mut root) { eprintln!("invalid --state-root hex"); return; }
                let tlv = crate::tlv::TlvMsg { version: crate::tlv::TLV_VERSION, msg_type: crate::tlv::MsgType::Checkpoint as u8, episode_id: episode_id as u64, seq, state_hash: root, payload: vec![] };
                send_with_ack(&dest, tlv, false, !no_ack);
                return;
            }
            if r#type == "close" {
                send_tlv_close(&dest, episode_id as u64, seq, !no_ack);
                return;
            }
            if r#type == "new" {
                // Include pubkey as authorized participant from CLI/env/dev.key if available
                let participants = if let Some(sk_hex) = resolve_dev_key_hex(&kaspa_private_key) {
                    let mut private_key_bytes = [0u8; 32];
                    if faster_hex::hex_decode(sk_hex.trim().as_bytes(), &mut private_key_bytes).is_err() {
                        eprintln!("invalid private key hex (dev key/env/flag)");
                        return;
                    }
                    let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).expect("invalid sk");
                    vec![kdapp::pki::PubKey(keypair.public_key())]
                } else { vec![] };
                let cmd = EpisodeMessage::<LotteryEpisode>::NewEpisode { episode_id, participants };
                send_tlv_new(&dest, episode_id as u64, seq, cmd, !no_ack);
                return;
            }
            // cmd
            let emsg = if let (Some(a), ns) = (amount, &numbers) {
                if ns.len() != 5 { eprintln!("provide exactly 5 numbers for --amount mode"); return; }
                let nums = [ns[0], ns[1], ns[2], ns[3], ns[4]];
                let cmd = LotteryCommand::BuyTicket { numbers: nums, entry_amount: a };
                if let Some(sk_hex) = resolve_dev_key_hex(&kaspa_private_key) {
                    let mut private_key_bytes = [0u8; 32];
                    if faster_hex::hex_decode(sk_hex.trim().as_bytes(), &mut private_key_bytes).is_err() { eprintln!("invalid private key hex (dev key/env/flag)"); return; }
                    let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).expect("invalid sk");
                    let pk = kdapp::pki::PubKey(keypair.public_key());
                    EpisodeMessage::<LotteryEpisode>::new_signed_command(episode_id, cmd, keypair.secret_key(), pk)
                } else { EpisodeMessage::<LotteryEpisode>::UnsignedCommand { episode_id, cmd } }
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

const FEE: u64 = 5_000;

enum SubmitKind {
    New { episode_id: u32 },
    Buy { episode_id: u32, amount: u64, numbers: [u8; 5] },
    Draw { episode_id: u32, entropy: String },
    Claim { episode_id: u32, ticket_id: u64, round: u64 },
}

async fn submit_tx_flow(kind: SubmitKind, sk_hex_opt: Option<String>, mainnet: bool, wrpc_url: Option<String>) {
    // Build signer + address
    let Some(sk_hex) = resolve_dev_key_hex(&sk_hex_opt) else {
        eprintln!("no private key provided: pass --kaspa-private-key, set KASPA_PRIVATE_KEY, KAS_DRAW_DEV_SK, or put dev key hex in examples/kas-draw/dev.key");
        return;
    };
    let mut private_key_bytes = [0u8; 32];
    if faster_hex::hex_decode(sk_hex.trim().as_bytes(), &mut private_key_bytes).is_err() { eprintln!("invalid private key hex"); return; }
    let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).expect("invalid sk");
    let network = if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
    let addr_prefix = if mainnet { AddrPrefix::Mainnet } else { AddrPrefix::Testnet };
    let addr = Address::new(addr_prefix, AddrVersion::PubKey, &keypair.x_only_public_key().0.serialize());
    info!("funding address: {}", addr);

    // Connect and fetch UTXOs
    let url_opt = get_wrpc_url(wrpc_url);
    let kaspad = kdapp::proxy::connect_client(network, url_opt).await.expect("kaspad connect");
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

fn get_wrpc_url(flag: Option<String>) -> Option<String> {
    if flag.is_some() {
        return flag;
    }
    std::env::var("WRPC_URL").ok()
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

// Encode OKCP v1 record
fn encode_okcp(episode_id: u64, seq: u64, root: [u8; 32]) -> Vec<u8> {
    use byteorder::{LittleEndian, WriteBytesExt};
    let mut rec = Vec::with_capacity(4 + 1 + 8 + 8 + 32);
    rec.extend_from_slice(b"OKCP");
    rec.push(1u8);
    let _ = rec.write_u64::<LittleEndian>(episode_id);
    let _ = rec.write_u64::<LittleEndian>(seq);
    rec.extend_from_slice(&root);
    rec
}

// Submit a checkpoint transaction carrying a KDCK payload
async fn submit_checkpoint_tx(episode_id: u64, seq: u64, root: [u8; 32], sk_hex_opt: Option<String>, mainnet: bool, wrpc_url: Option<String>) -> Result<(), String> {
    // Build signer + address
    let sk_hex = resolve_dev_key_hex(&sk_hex_opt).ok_or_else(|| "no private key provided (flag/env/file)".to_string())?;
    let mut private_key_bytes = [0u8; 32];
    faster_hex::hex_decode(sk_hex.trim().as_bytes(), &mut private_key_bytes).map_err(|_| "invalid private key hex".to_string())?;
    let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).map_err(|_| "invalid sk".to_string())?;
    let network = if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
    let addr_prefix = if mainnet { AddrPrefix::Mainnet } else { AddrPrefix::Testnet };
    let addr = Address::new(addr_prefix, AddrVersion::PubKey, &keypair.x_only_public_key().0.serialize());

    // Connect and fetch UTXOs
    let url_opt = get_wrpc_url(wrpc_url);
    let kaspad = kdapp::proxy::connect_client(network, url_opt).await.map_err(|e| e.to_string())?;
    let utxos = kaspad
        .get_utxos_by_addresses(vec![addr.clone()])
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|u| (kaspa_consensus_core::tx::TransactionOutpoint::from(u.outpoint), kaspa_consensus_core::tx::UtxoEntry::from(u.utxo_entry)))
        .collect::<Vec<_>>();
    if utxos.is_empty() { return Err(format!("no UTXOs for {}", addr)); }
    let (op, entry) = utxos.iter().max_by_key(|(_, e)| e.amount).cloned().unwrap();
    if entry.amount <= FEE { return Err(format!("selected UTXO too small: {}", entry.amount)); }

    // Build payload: OKCP record
    let payload = encode_okcp(episode_id, seq, root);
    // Use a dedicated prefix for checkpoints (KDCK)
    const CHECKPOINT_PREFIX: PrefixType = u32::from_le_bytes(*b"KDCK");
    let gen = TransactionGenerator::new(keypair, PATTERN, CHECKPOINT_PREFIX);
    let tx = gen.build_raw_payload_transaction((op, entry), &addr, &payload, FEE);
    submit_tx_retry(&kaspad, &tx, 3).await
}

fn seq_store_path() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from("target");
    let _ = std::fs::create_dir_all(&p);
    p.push("kas_draw_offchain_seq.txt");
    p
}

fn read_seq_store() -> std::collections::HashMap<u64, u64> {
    use std::io::Read;
    let mut m = std::collections::HashMap::new();
    let path = seq_store_path();
    if let Ok(mut f) = std::fs::File::open(path) {
        let mut s = String::new();
        if f.read_to_string(&mut s).is_ok() {
            for line in s.lines() {
                let parts: Vec<&str> = line.trim().split(',').collect();
                if parts.len() == 2 {
                    if let (Ok(eid), Ok(seq)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) {
                        m.insert(eid, seq);
                    }
                }
            }
        }
    }
    m
}

fn write_seq_store(m: &std::collections::HashMap<u64, u64>) {
    use std::io::Write;
    let mut s = String::new();
    let mut keys: Vec<_> = m.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let v = m.get(&k).copied().unwrap_or(0);
        s.push_str(&format!("{},{}\n", k, v));
    }
    let path = seq_store_path();
    if let Ok(mut f) = std::fs::File::create(path) {
        let _ = f.write_all(s.as_bytes());
    }
}

fn auto_seq(episode_id: u64, typ: &str) -> u64 {
    let mut store = read_seq_store();
    let next = match (store.get(&episode_id).copied(), typ) {
        (Some(last), _) => last.saturating_add(1),
        (None, "new") => 0,
        _ => 1,
    };
    store.insert(episode_id, next);
    write_seq_store(&store);
    next
}

// Resolve a development private key hex for convenience testing:
// Order: explicit CLI flag -> env KASPA_PRIVATE_KEY -> env KAS_DRAW_DEV_SK -> dev.key files
fn resolve_dev_key_hex(cli_opt: &Option<String>) -> Option<String> {
    if let Some(s) = cli_opt.as_ref() { return Some(s.clone()); }
    if let Ok(s) = std::env::var("KASPA_PRIVATE_KEY") { if !s.trim().is_empty() { return Some(s); } }
    if let Ok(s) = std::env::var("KAS_DRAW_DEV_SK") { if !s.trim().is_empty() { return Some(s); } }
    // Try common dev file locations (gitignored by **/*.key)
    let candidates = [
        "examples/kas-draw/dev.key",
        "examples/comment-board/dev.key",
        "dev.key",
        ".dev.key",
    ];
    for path in candidates {
        if let Ok(s) = std::fs::read_to_string(path) { let t = s.trim().to_string(); if !t.is_empty() { return Some(t); } }
    }
    None
}

fn send_tlv_cmd(dest: &str, episode_id: u64, seq: u64, msg: EpisodeMessage<LotteryEpisode>, wait_ack: bool) {
    let payload = borsh::to_vec(&msg).unwrap();
    // State hash is opaque to router; handler recomputes actual state after applying
    let tlv = crate::tlv::TlvMsg { version: crate::tlv::TLV_VERSION, msg_type: crate::tlv::MsgType::Cmd as u8, episode_id, seq, state_hash: [0u8; 32], payload };
    send_with_ack(dest, tlv, false, wait_ack);
}

fn send_tlv_new(dest: &str, episode_id: u64, seq: u64, msg: EpisodeMessage<LotteryEpisode>, wait_ack: bool) {
    let payload = borsh::to_vec(&msg).unwrap();
    let tlv = crate::tlv::TlvMsg { version: crate::tlv::TLV_VERSION, msg_type: crate::tlv::MsgType::New as u8, episode_id, seq, state_hash: [0u8; 32], payload };
    send_with_ack(dest, tlv, false, wait_ack);
}

fn send_tlv_close(dest: &str, episode_id: u64, seq: u64, wait_ack: bool) {
    let tlv = crate::tlv::TlvMsg { version: crate::tlv::TLV_VERSION, msg_type: crate::tlv::MsgType::Close as u8, episode_id, seq, state_hash: [0u8; 32], payload: vec![] };
    send_with_ack(dest, tlv, true, wait_ack);
}

fn send_with_ack(dest: &str, tlv: crate::tlv::TlvMsg, expect_close_ack: bool, wait_ack: bool) {
    use std::net::UdpSocket;
    use std::time::Duration;
    let sock = UdpSocket::bind("0.0.0.0:0").expect("bind sender");
    let expected_type = if expect_close_ack { crate::tlv::MsgType::AckClose as u8 } else { crate::tlv::MsgType::Ack as u8 };

    let attempts = if wait_ack { 3 } else { 1 };
    let mut timeout_ms = 300u64;
    let bytes = tlv.encode();
    for attempt in 0..attempts {
        let _ = sock.send_to(&bytes, dest);
        if !wait_ack { break; }
        let _ = sock.set_read_timeout(Some(Duration::from_millis(timeout_ms)));
        let mut buf = [0u8; 1024];
        if let Ok((n, _from)) = sock.recv_from(&mut buf) {
            if let Some(ack) = crate::tlv::TlvMsg::decode(&buf[..n]) {
                if ack.msg_type == expected_type && ack.episode_id == tlv.episode_id && ack.seq == tlv.seq {
                    println!("ack received for ep {} seq {}", tlv.episode_id, tlv.seq);
                    return;
                }
            }
        }
        if attempt + 1 < attempts {
            timeout_ms = timeout_ms.saturating_mul(2);
            println!("ack timeout, retrying (attempt {} of {})", attempt + 2, attempts);
        } else {
            eprintln!("ack failed for ep {} seq {} (no response)", tlv.episode_id, tlv.seq);
        }
    }
}
