use std::net::UdpSocket;

use kaspa_addresses::{Address, Prefix as AddrPrefix, Version as AddrVersion};
use kaspa_consensus_core::{
    network::{NetworkId, NetworkType},
    tx::{TransactionOutpoint, UtxoEntry},
};
use kaspa_wrpc_client::prelude::*;
use kdapp::{
    generator::{PatternType, PrefixType, TransactionGenerator},
    proxy,
};
use log::{info, warn};
use secp256k1::Keypair;

use crate::tlv::{MsgType, TlvMsg, DEMO_HMAC_KEY};

const FEE: u64 = 5_000;
const CHECKPOINT_PREFIX: PrefixType = u32::from_le_bytes(*b"KMCP");

fn pattern() -> PatternType { [(0u8, 0u8); 10] }

fn encode_okcp(episode_id: u64, seq: u64, root: [u8; 32]) -> Vec<u8> {
let mut rec = Vec::with_capacity(4 + 1 + 8 + 8 + 32);
rec.extend_from_slice(b"OKCP");
rec.push(1u8);
rec.extend_from_slice(&episode_id.to_le_bytes());
rec.extend_from_slice(&seq.to_le_bytes());
rec.extend_from_slice(&root);
rec
}

async fn submit_checkpoint_tx(
    episode_id: u64,
    seq: u64,
    root: [u8; 32],
    sk_hex: &str,
    mainnet: bool,
    wrpc_url: Option<String>,
) -> Result<(), String> {
    let mut sk_bytes = [0u8; 32];
    faster_hex::hex_decode(sk_hex.trim().as_bytes(), &mut sk_bytes)
        .map_err(|_| "invalid private key hex".to_string())?;
    let keypair =
        Keypair::from_seckey_slice(secp256k1::SECP256K1, &sk_bytes).map_err(|_| "invalid sk".to_string())?;
    let network =
        if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
    let addr_prefix = if mainnet { AddrPrefix::Mainnet } else { AddrPrefix::Testnet };
    let addr = Address::new(addr_prefix, AddrVersion::PubKey, &keypair.x_only_public_key().0.serialize());

    let kaspad = proxy::connect_client(network, wrpc_url)
        .await
        .map_err(|e| e.to_string())?;
    let utxos = kaspad
        .get_utxos_by_addresses(vec![addr.clone()])
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|u| {
            (
                TransactionOutpoint::from(u.outpoint),
                UtxoEntry::from(u.utxo_entry),
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
    let gen = TransactionGenerator::new(keypair, pattern(), CHECKPOINT_PREFIX);
    let send = entry.amount - FEE;
    let tx = gen.build_transaction(&[(op, entry)], send, 1, &addr, payload);
    submit_tx_retry(&kaspad, &tx, 3).await
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
                if msg.contains("WebSocket") || msg.contains("not connected") || msg.contains("disconnected") {
                    let _ = kaspad.connect(Some(proxy::connect_options())).await;
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

pub fn run(
    bind: &str,
    kaspa_private_key: String,
    mainnet: bool,
    wrpc_url: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let sock = UdpSocket::bind(bind)?;
    info!("watcher listening on {bind}");
    let rt = tokio::runtime::Runtime::new()?;
    let mut buf = [0u8; 1024];
    loop {
        let (n, src) = sock.recv_from(&mut buf)?;
        let Some(msg) = TlvMsg::decode(&buf[..n]) else {
            warn!("watcher: invalid TLV from {src}");
            continue;
        };
        if msg.msg_type != MsgType::Checkpoint as u8 || !msg.verify(DEMO_HMAC_KEY) {
            warn!("watcher: ignored msg from {src}");
            continue;
        }
        let root = msg.state_hash;
        let ep = msg.episode_id;
        let seq = msg.seq;
        info!("checkpoint received: ep={ep} seq={seq}");
        let key = kaspa_private_key.clone();
        let url = wrpc_url.clone();
        if let Err(e) = rt.block_on(submit_checkpoint_tx(ep, seq, root, &key, mainnet, url)) {
            warn!("anchor failed: {e}");
        } else {
            info!("anchor submitted for ep={ep} seq={seq}");
        }
    }
}

