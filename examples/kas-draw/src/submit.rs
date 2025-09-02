use kaspa_addresses::{Address, Prefix as AddrPrefix, Version as AddrVersion};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_wrpc_client::prelude::*;
use kdapp::engine::EpisodeMessage;
use kdapp::generator::TransactionGenerator;
use kdapp::pki::PubKey;
use log::info;
use secp256k1::Keypair;

use crate::episode::{LotteryCommand, LotteryEpisode};
use crate::routing;

pub const FEE: u64 = 5_000;

pub enum SubmitKind {
    New { episode_id: u32 },
    Buy { episode_id: u32, amount: u64, numbers: [u8; 5] },
    Draw { episode_id: u32, entropy: String },
    Claim { episode_id: u32, ticket_id: u64, round: u64 },
}

pub async fn submit_tx_flow(
    kind: SubmitKind,
    sk_hex_opt: Option<String>,
    mainnet: bool,
    wrpc_url: Option<String>,
) {
    // Build signer + address
    let Some(sk_hex) = resolve_dev_key_hex(&sk_hex_opt) else {
        eprintln!("no private key provided: pass --kaspa-private-key, set KASPA_PRIVATE_KEY, KAS_DRAW_DEV_SK, or put dev key hex in examples/kas-draw/dev.key");
        return;
    };
    let mut private_key_bytes = [0u8; 32];
    if faster_hex::hex_decode(sk_hex.trim().as_bytes(), &mut private_key_bytes).is_err() {
        eprintln!("invalid private key hex");
        return;
    }
    let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).expect("invalid sk");
    let network = if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
    let addr_prefix = if mainnet { AddrPrefix::Mainnet } else { AddrPrefix::Testnet };
    let addr = Address::new(addr_prefix, AddrVersion::PubKey, &keypair.x_only_public_key().0.serialize());
    info!("funding address: {addr}");

    // Connect and fetch UTXOs
    let url_opt = get_wrpc_url(wrpc_url);
    let kaspad = kdapp::proxy::connect_client(network, url_opt).await.expect("kaspad connect");
    let utxos = kaspad
        .get_utxos_by_addresses(vec![addr.clone()])
        .await
        .expect("get utxos")
        .into_iter()
        .map(|u| {
            (
                kaspa_consensus_core::tx::TransactionOutpoint::from(u.outpoint),
                kaspa_consensus_core::tx::UtxoEntry::from(u.utxo_entry),
            )
        })
        .collect::<Vec<_>>();
    if utxos.is_empty() {
        eprintln!("no UTXOs for {addr}");
        return;
    }
    // Pick the largest UTXO
    let (op, entry) = utxos.iter().max_by_key(|(_, e)| e.amount).expect("has utxo").clone();
    if entry.amount <= FEE {
        eprintln!("selected UTXO too small: {}", entry.amount);
        return;
    }

    // Build command payload
    let pk = PubKey(keypair.public_key());
    let msg = match kind {
        SubmitKind::New { episode_id } => EpisodeMessage::<LotteryEpisode>::NewEpisode {
            episode_id,
            participants: vec![pk],
        },
        SubmitKind::Buy {
            episode_id,
            amount,
            numbers,
        } => {
            let cmd = LotteryCommand::BuyTicket { numbers, entry_amount: amount };
            EpisodeMessage::<LotteryEpisode>::new_signed_command(episode_id, cmd, keypair.secret_key(), pk)
        }
        SubmitKind::Draw { episode_id, entropy } => {
            let cmd = LotteryCommand::ExecuteDraw { entropy_source: entropy };
            EpisodeMessage::<LotteryEpisode>::UnsignedCommand { episode_id, cmd }
        }
        SubmitKind::Claim {
            episode_id,
            ticket_id,
            round,
        } => {
            let cmd = LotteryCommand::ClaimPrize { ticket_id, round };
            EpisodeMessage::<LotteryEpisode>::UnsignedCommand { episode_id, cmd }
        }
    };

    // Build and submit transaction carrying the payload
    let gen = TransactionGenerator::new(keypair, routing::pattern(), routing::PREFIX);
    let tx = gen.build_command_transaction((op, entry), &addr, &msg, FEE);
    info!("built tx {} (payload)", tx.id());
    if let Err(e) = submit_tx_retry(&kaspad, &tx, 3).await {
        eprintln!("submit failed: {e}");
    } else {
        info!("submitted {}", tx.id());
    }
}

pub fn get_wrpc_url(flag: Option<String>) -> Option<String> {
    if flag.is_some() {
        return flag;
    }
    std::env::var("WRPC_URL").ok()
}

async fn submit_tx_retry(
    kaspad: &KaspaRpcClient,
    tx: &kaspa_consensus_core::tx::Transaction,
    attempts: usize,
) -> Result<(), String> {
    let mut tries = 0usize;
    loop {
        match kaspad.submit_transaction(tx.into(), false).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                tries += 1;
                let msg = e.to_string();
                if tries >= attempts {
                    return Err(format!("submit failed after {tries} attempts: {msg}"));
                }
                if msg.contains("WebSocket")
                    || msg.contains("not connected")
                    || msg.contains("disconnected")
                {
                    let _ = kaspad.connect(Some(kdapp::proxy::connect_options())).await;
                    continue;
                } else if msg.contains("orphan") {
                    continue;
                } else if msg.contains("already accepted") {
                    return Ok(());
                } else {
                    return Err(format!("submit failed: {msg}"));
                }
            }
        }
    }
}

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

pub async fn submit_checkpoint_tx(
    episode_id: u64,
    seq: u64,
    root: [u8; 32],
    sk_hex_opt: Option<String>,
    mainnet: bool,
    wrpc_url: Option<String>,
) -> Result<(), String> {
    let sk_hex = resolve_dev_key_hex(&sk_hex_opt).ok_or_else(|| "no private key provided (flag/env/file)".to_string())?;
    let mut private_key_bytes = [0u8; 32];
    faster_hex::hex_decode(sk_hex.trim().as_bytes(), &mut private_key_bytes)
        .map_err(|_| "invalid private key hex".to_string())?;
    let keypair =
        Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).map_err(|_| "invalid sk".to_string())?;
    let network = if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
    let addr_prefix = if mainnet { AddrPrefix::Mainnet } else { AddrPrefix::Testnet };
    let addr = Address::new(addr_prefix, AddrVersion::PubKey, &keypair.x_only_public_key().0.serialize());

    let url_opt = get_wrpc_url(wrpc_url);
    let kaspad = kdapp::proxy::connect_client(network, url_opt)
        .await
        .map_err(|e| e.to_string())?;
    let utxos = kaspad
        .get_utxos_by_addresses(vec![addr.clone()])
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|u| {
            (
                kaspa_consensus_core::tx::TransactionOutpoint::from(u.outpoint),
                kaspa_consensus_core::tx::UtxoEntry::from(u.utxo_entry),
            )
        })
        .collect::<Vec<_>>();
    if utxos.is_empty() {
        return Err(format!("no UTXOs for {addr}"));
    }
    let (op, entry) = utxos.iter().max_by_key(|(_, e)| e.amount).cloned().unwrap();
    if entry.amount <= FEE {
        return Err(format!("selected UTXO too small: {}", entry.amount));
    }

    let payload = encode_okcp(episode_id, seq, root);
    let gen = TransactionGenerator::new(keypair, routing::pattern(), routing::CHECKPOINT_PREFIX);
    let send = entry.amount - FEE;
    let tx = gen.build_transaction(&[(op, entry)], send, 1, &addr, payload);
    submit_tx_retry(&kaspad, &tx, 3).await
}

pub fn resolve_dev_key_hex(cli_opt: &Option<String>) -> Option<String> {
    if let Some(s) = cli_opt.as_ref() {
        return Some(s.clone());
    }
    if let Ok(s) = std::env::var("KASPA_PRIVATE_KEY") {
        if !s.trim().is_empty() {
            return Some(s);
        }
    }
    if let Ok(s) = std::env::var("KAS_DRAW_DEV_SK") {
        if !s.trim().is_empty() {
            return Some(s);
        }
    }
    let candidates = [
        "examples/kas-draw/dev.key",
        "examples/comment-board/dev.key",
        "dev.key",
        ".dev.key",
    ];
    for path in candidates {
        if let Ok(s) = std::fs::read_to_string(path) {
            let t = s.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    if std::env::var("KAS_DRAW_USE_TEST_KEY").ok().as_deref() == Some("1") {
        return Some("7f7c92f0382d3d02f3e0d5d1446f2e4e5a0f6aa8a8c9f2d7b2a1c0f9e8d7c6b5".to_string());
    }
    None
}

