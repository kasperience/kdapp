use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::client::KaspaRpcClient;
use kdapp::pki::generate_keypair;
use kdapp::proxy;
use log::{info, warn};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::Deserialize;
use clap::Parser;
use faster_hex::{hex_decode, hex_encode};

use crate::{receive, GuardianMsg, GuardianState, DEMO_HMAC_KEY, TlvMsg, TLV_VERSION};
use kdapp::pki::PubKey;

#[derive(Clone, Debug, Deserialize)]
pub struct GuardianConfig {
    pub listen_addr: String,
    pub wrpc_url: Option<String>,
    #[serde(default)]
    pub mainnet: bool,
    pub key_path: PathBuf,
}

#[derive(Parser, Debug)]
#[command(name = "guardian-service", about = "kdapp guardian service")]
struct Cli {
    /// Path to a TOML configuration file
    #[arg(long)]
    config: Option<PathBuf>,
    /// UDP listen address
    #[arg(long)]
    listen_addr: Option<String>,
    /// Optional wRPC endpoint
    #[arg(long)]
    wrpc_url: Option<String>,
    /// Use mainnet (default testnet-10)
    #[arg(long, default_value_t = false)]
    mainnet: bool,
    /// Path to guardian private key
    #[arg(long)]
    key_path: Option<PathBuf>,
}

impl GuardianConfig {
    pub fn from_args() -> Self {
        let args = Cli::parse();
        let mut cfg = if let Some(path) = args.config {
            let text = fs::read_to_string(path).expect("read config");
            toml::from_str(&text).expect("parse config")
        } else {
            GuardianConfig {
                listen_addr: "127.0.0.1:9650".to_string(),
                wrpc_url: None,
                mainnet: false,
                key_path: PathBuf::from("guardian.key"),
            }
        };

        if let Some(v) = args.listen_addr {
            cfg.listen_addr = v;
        }
        if let Some(v) = args.wrpc_url {
            cfg.wrpc_url = Some(v);
        }
        if args.mainnet {
            cfg.mainnet = true;
        }
        if let Some(v) = args.key_path {
            cfg.key_path = v;
        }
        cfg
    }
}

#[derive(Clone, Debug)]
struct OkcpRecord {
    episode_id: u64,
    seq: u64,
}

fn decode_okcp(bytes: &[u8]) -> Option<OkcpRecord> {
    const MIN_LEN: usize = 4 + 1 + 8 + 8 + 32;
    if bytes.len() < MIN_LEN {
        return None;
    }
    if &bytes[0..4] != b"OKCP" || bytes[4] != 1 {
        return None;
    }
    let pid_start = 5;
    let pid_end = pid_start + 8;
    let seq_end = pid_end + 8;
    let episode_id = u64::from_le_bytes(bytes[pid_start..pid_end].try_into().ok()?);
    let seq = u64::from_le_bytes(bytes[pid_end..seq_end].try_into().ok()?);
    Some(OkcpRecord { episode_id, seq })
}

async fn watch_anchors(client: &KaspaRpcClient, state: Arc<Mutex<GuardianState>>) -> Result<(), Box<dyn std::error::Error>> {
    // Poll the virtual chain and scan merged blocks for OKCP payloads
    use tokio::time::{sleep, Duration};

    let info = client.get_block_dag_info().await?;
    let mut sink = info.sink;
    loop {
        let vcb = match client.get_virtual_chain_from_block(sink, true).await {
            Ok(v) => v,
            Err(e) => {
                // Try to reconnect and continue
                warn!("guardian: failed to fetch virtual chain: {e}");
                let _ = client.connect(Some(kdapp::proxy::connect_options())).await;
                sleep(Duration::from_millis(500)).await;
                continue;
            }
        };

        if let Some(new_sink) = vcb.accepted_transaction_ids.last().map(|ncb| ncb.accepting_block_hash) {
            sink = new_sink;
        } else {
            sleep(Duration::from_millis(500)).await;
            continue;
        }

        for ncb in vcb.accepted_transaction_ids {
            let accepting_hash = ncb.accepting_block_hash;
            let accepting_block = match client.get_block(accepting_hash, false).await {
                Ok(b) => b,
                Err(_) => continue,
            };
            let Some(verbose) = accepting_block.verbose_data else { continue };
            // Iterate merged blocks and inspect transactions for OKCP payloads
            for merged_hash in verbose.merge_set_blues_hashes.into_iter().chain(verbose.merge_set_reds_hashes.into_iter()) {
                let merged = match client.get_block(merged_hash, true).await {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                for tx in merged.transactions {
                    let payload = &tx.payload;
                    if !payload.is_empty() {
                        if let Some(rec) = decode_okcp(payload) {
                            let mut s = state.lock().unwrap();
                            if s.record_checkpoint(rec.episode_id, rec.seq) {
                                drop(s);
                                handle_escalate(&state, rec.episode_id, None);
                            }
                        }
                    }
                }
            }
        }
    }
}

const REFUND_TYPE: u8 = 7;
const WATCHER_ADDR: &str = "127.0.0.1:9590";

fn handle_escalate(state: &Arc<Mutex<GuardianState>>, episode_id: u64, refund_tx: Option<Vec<u8>>) {
    let discrep = { state.lock().unwrap().disputes.contains(&episode_id) };
    if !discrep {
        warn!("guardian: no checkpoint discrepancy for episode {episode_id}");
        return;
    }
    if let Some(tx) = refund_tx {
        if let Some(sk) = GUARDIAN_SK.get() {
            let sig = { let mut s = state.lock().unwrap(); s.sign_refund(episode_id, &tx, sk) };
            let secp = Secp256k1::new();
            let gpk = PubKey(PublicKey::from_secret_key(&secp, sk));
            let payload = borsh::to_vec(&(tx, sig, gpk)).expect("serialize refund");
            let mut msg = TlvMsg { version: TLV_VERSION, msg_type: REFUND_TYPE, episode_id, seq: 0, state_hash: [0u8; 32], payload, auth: [0u8; 32] };
            msg.sign(DEMO_HMAC_KEY);
            let sock = UdpSocket::bind("0.0.0.0:0").expect("bind refund sender");
            let _ = sock.send_to(&msg.encode(), WATCHER_ADDR);
            info!("guardian: co-signed refund for episode {episode_id}");
        }
    }
}

static STARTED: OnceLock<()> = OnceLock::new();
static GUARDIAN_SK: OnceLock<SecretKey> = OnceLock::new();

fn parse_secret_key(hex: &str) -> Option<SecretKey> {
    let mut buf = [0u8; 32];
    let mut tmp = vec![0u8; hex.len() / 2 + hex.len() % 2];
    if hex_decode(hex.as_bytes(), &mut tmp).is_ok() && tmp.len() == 32 {
        buf.copy_from_slice(&tmp);
        SecretKey::from_slice(&buf).ok()
    } else {
        None
    }
}

fn secret_key_to_hex(sk: &SecretKey) -> String {
    let bytes = sk.secret_bytes();
    let mut out = vec![0u8; bytes.len() * 2];
    hex_encode(&bytes, &mut out).expect("hex encode");
    String::from_utf8(out).expect("utf8")
}

fn load_or_generate_key(path: &Path) -> SecretKey {
    if let Ok(text) = fs::read_to_string(path) {
        if let Some(sk) = parse_secret_key(text.trim()) {
            return sk;
        }
    }
    let (sk, _) = generate_keypair();
    if let Err(e) = fs::write(path, secret_key_to_hex(&sk)) {
        warn!("guardian: failed to write key to {}: {e}", path.display());
    }
    sk
}

pub fn run(config: &GuardianConfig) -> Arc<Mutex<GuardianState>> {
    STARTED.get_or_init(|| {});
    let sk = GUARDIAN_SK.get_or_init(|| load_or_generate_key(&config.key_path));
    let secp = Secp256k1::new();
    let pk = PublicKey::from_secret_key(&secp, sk);
    let bytes = pk.serialize();
    let mut pk_hex = vec![0u8; bytes.len() * 2];
    hex_encode(&bytes, &mut pk_hex).expect("hex encode");
    info!("guardian public key {}", String::from_utf8(pk_hex).expect("utf8"));
    let sock = UdpSocket::bind(&config.listen_addr).expect("bind guardian service");
    info!("guardian service listening on {}", config.listen_addr);
    let state = Arc::new(Mutex::new(GuardianState::default()));

    // spawn UDP listener
    let sock_clone = sock.try_clone().expect("clone socket");
    let state_clone = state.clone();
    thread::spawn(move || loop {
        let mut st = state_clone.lock().unwrap();
        if let Some(GuardianMsg::Escalate { episode_id, refund_tx, .. }) = receive(&sock_clone, &mut st, DEMO_HMAC_KEY) {
            drop(st);
            handle_escalate(&state_clone, episode_id, Some(refund_tx));
        }
    });

    // spawn wRPC watcher
    let state_watch = state.clone();
    let wrpc_url = config.wrpc_url.clone();
    let mainnet = config.mainnet;
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(async move {
            let network = if mainnet {
                NetworkId::new(NetworkType::Mainnet)
            } else {
                NetworkId::with_suffix(NetworkType::Testnet, 10)
            };
            if let Ok(client) = proxy::connect_client(network, wrpc_url).await {
                let _ = watch_anchors(&client, state_watch).await;
            }
        });
    });

    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_without_discrepancy() {
        let _ = GUARDIAN_SK.set(SecretKey::from_slice(&[1u8; 32]).unwrap());
        let state = Arc::new(Mutex::new(GuardianState::default()));
        handle_escalate(&state, 1, Some(b"tx".to_vec()));
        assert!(state.lock().unwrap().refund_signatures.is_empty());
    }
}
