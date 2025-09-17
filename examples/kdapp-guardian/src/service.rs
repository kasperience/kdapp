use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use axum::{extract::State, routing::get, Json, Router};
use clap::Parser;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::client::KaspaRpcClient;
use kdapp::pki::PubKey;
use kdapp::proxy;
use log::{info, warn};
use ripemd::Ripemd160;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    clear_handshake_store,
    handshake_db::HandshakeStore,
    keys::{GuardianKeySource, GuardianKeyStorage},
    metrics,
    receive,
    set_handshake_store,
    GuardianMsg,
    GuardianState,
    DEMO_HMAC_KEY,
};

#[derive(Clone, Debug, Deserialize)]
pub struct GuardianConfig {
    pub listen_addr: String,
    pub wrpc_url: String,
    pub mainnet: bool,
    pub key_path: String,
    pub state_path: Option<PathBuf>,
    pub log_level: String,
    pub http_port: u16,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9735".to_string(),
            wrpc_url: "ws://127.0.0.1:16110".to_string(),
            mainnet: false,
            key_path: "guardian.key".to_string(),
            state_path: Some(PathBuf::from("guardian.state")),
            log_level: "info".to_string(),
            http_port: 9651,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "guardian-service", about = "kdapp guardian service")]
pub struct Cli {
    #[arg(long)]
    pub listen_addr: Option<String>,
    #[arg(long)]
    pub wrpc_url: Option<String>,
    #[arg(long, default_value_t = false)]
    pub mainnet: bool,
    #[arg(long)]
    pub key_path: Option<String>,
    #[arg(long)]
    pub state_path: Option<PathBuf>,
    #[arg(long, default_value = "info")]
    pub log_level: String,
    #[arg(long)]
    pub http_port: Option<u16>,
    #[arg(long)]
    pub config: Option<String>,
}

impl Cli {
    pub fn merge_into_config(self, mut cfg: GuardianConfig) -> GuardianConfig {
        if let Some(v) = self.listen_addr {
            cfg.listen_addr = v;
        }
        if let Some(v) = self.wrpc_url {
            cfg.wrpc_url = v;
        }
        if self.mainnet {
            cfg.mainnet = true;
        }
        if let Some(v) = self.key_path {
            cfg.key_path = v;
        }
        if let Some(v) = self.state_path {
            cfg.state_path = Some(v);
        }
        cfg.log_level = self.log_level;
        if let Some(p) = self.http_port {
            cfg.http_port = p;
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

#[derive(Serialize)]
struct HttpMetrics {
    valid: u64,
    invalid: u64,
    disputes_open: usize,
    disputes_closed: usize,
    refunds_signed: usize,
}

async fn healthz() -> &'static str {
    "ok"
}

async fn metrics_endpoint(State(state): State<Arc<Mutex<GuardianState>>>) -> Json<HttpMetrics> {
    let (valid, invalid) = metrics::snapshot();
    let st = state.lock().unwrap();
    Json(HttpMetrics {
        valid,
        invalid,
        disputes_open: st.disputes.len(),
        disputes_closed: st.checkpoints.len(),
        refunds_signed: st.refund_signatures.len(),
    })
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
                            let discrepancy = s.record_checkpoint(rec.episode_id, rec.seq);
                            drop(s);
                            if discrepancy {
                                handle_escalate(&state, rec.episode_id, None);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn handle_escalate(state: &Arc<Mutex<GuardianState>>, episode_id: u64, refund_tx: Option<Vec<u8>>) {
    if let Some(tx) = refund_tx {
        if let Some(sk) = GUARDIAN_SK.get() {
            let mut s = state.lock().unwrap();
            let _sig = s.sign_refund(episode_id, &tx, sk);
            info!("guardian: co-signed refund for episode {episode_id}");
        }
    } else {
        info!("guardian: discrepancy detected for episode {episode_id}");
    }
}

static STARTED: OnceLock<()> = OnceLock::new();
static GUARDIAN_SK: OnceLock<SecretKey> = OnceLock::new();

pub fn current_guardian_pubkey() -> Option<PubKey> {
    let secp = Secp256k1::new();
    GUARDIAN_SK.get().map(|sk| PubKey(PublicKey::from_secret_key(&secp, sk)))
}

fn pubkey_fingerprint(pk: &PublicKey) -> String {
    let compressed = pk.serialize();
    let sha = Sha256::digest(compressed);
    let ripe = Ripemd160::digest(sha);
    hex::encode(ripe)
}

/// Handle returned by [`run`] to allow graceful shutdown in tests.
pub struct ServiceHandle {
    pub state: Arc<Mutex<GuardianState>>,
    pub handshake_store: Arc<HandshakeStore>,
    shutdown: Arc<AtomicBool>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl ServiceHandle {
    pub fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        while let Some(handle) = self.threads.pop() {
            let _ = handle.join();
        }
    }
}

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        while let Some(handle) = self.threads.pop() {
            let _ = handle.join();
        }
        let _ = self.handshake_store.flush();
        clear_handshake_store();
    }
}

pub fn run(cfg: GuardianConfig) -> ServiceHandle {
    STARTED.get_or_init(|| {});
    let key_source = GuardianKeySource::from_uri(&cfg.key_path);
    let key_desc = key_source.describe().to_string();
    let was_uninitialized = GUARDIAN_SK.get().is_none();
    let sk = GUARDIAN_SK.get_or_init({
        let key_source = key_source.clone();
        let key_desc = key_desc.clone();
        move || {
            key_source
                .load_key()
                .unwrap_or_else(|err| panic!("guardian: failed to load {key_desc}: {err}"))
        }
    });
    if was_uninitialized {
        info!("Guardian signing backend: {key_desc}");
    }
    let secp = Secp256k1::new();
    let pk = PublicKey::from_secret_key(&secp, sk);
    info!("Guardian pubkey: {}", hex::encode(pk.serialize()));
    info!("Guardian fingerprint: {}", pubkey_fingerprint(&pk));
    info!("Listen: {}  wRPC: {}  mainnet: {}", cfg.listen_addr, cfg.wrpc_url, cfg.mainnet);
    let sock = UdpSocket::bind(&cfg.listen_addr).expect("bind guardian service");
    sock.set_nonblocking(true).expect("nonblocking");
    let state_path = cfg.state_path.clone().unwrap_or_else(|| PathBuf::from("guardian.state"));
    let state = Arc::new(Mutex::new(GuardianState::load(&state_path)));
    let handshake_path = state_path.with_extension("handshake.db");
    let handshake_store = Arc::new(
        HandshakeStore::open(&handshake_path).unwrap_or_else(|err| {
            panic!(
                "guardian: failed to open handshake db {}: {err}",
                handshake_path.display()
            )
        }),
    );
    set_handshake_store(handshake_store.clone());

    let shutdown = Arc::new(AtomicBool::new(false));
    let mut threads = Vec::new();

    // spawn UDP listener
    let sock_clone = sock.try_clone().expect("clone socket");
    let state_clone = state.clone();
    let shutdown_udp = shutdown.clone();
    let udp_handle = thread::spawn(move || {
        let _ = sock_clone.set_read_timeout(Some(Duration::from_millis(200)));
        loop {
            if shutdown_udp.load(Ordering::Relaxed) {
                break;
            }
            let mut st = state_clone.lock().unwrap();
            if let Some(GuardianMsg::Escalate { episode_id, refund_tx, .. }) = receive(&sock_clone, &mut st, DEMO_HMAC_KEY) {
                drop(st);
                handle_escalate(&state_clone, episode_id, Some(refund_tx));
            }
        }
    });
    threads.push(udp_handle);

    // spawn wRPC watcher
    let state_watch = state.clone();
    let wrpc_url = Some(cfg.wrpc_url.clone());
    let mainnet = cfg.mainnet;
    let shutdown_watch = shutdown.clone();
    let wrpc_handle = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(async move {
            let network =
                if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
            if let Ok(client) = proxy::connect_client(network, wrpc_url).await {
                let shutdown_fut = async move {
                    while !shutdown_watch.load(Ordering::Relaxed) {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                };
                tokio::select! {
                    _ = watch_anchors(&client, state_watch) => {},
                    _ = shutdown_fut => {},
                }
            } else {
                while !shutdown_watch.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });
    });
    threads.push(wrpc_handle);

    // spawn HTTP server for health and metrics
    let state_http = state.clone();
    let http_addr = cfg
        .listen_addr
        .rsplit_once(':')
        .map(|(host, _)| format!("{}:{}", host, cfg.http_port))
        .unwrap_or_else(|| format!("127.0.0.1:{}", cfg.http_port));
    let shutdown_http = shutdown.clone();
    let http_handle = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(async move {
            let app = Router::new().route("/healthz", get(healthz)).route("/metrics", get(metrics_endpoint)).with_state(state_http);
            let shutdown_fut = async move {
                while !shutdown_http.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            };
            if let Ok(listener) = tokio::net::TcpListener::bind(http_addr).await {
                let _ = axum::serve(listener, app).with_graceful_shutdown(shutdown_fut).await;
            } else {
                shutdown_fut.await;
            }
        });
    });
    threads.push(http_handle);

    ServiceHandle { state, handshake_store, shutdown, threads }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdapp::pki::generate_keypair;
    use std::sync::{Mutex, OnceLock};

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn signs_refund() {
        let _g = test_guard();
        let state = Arc::new(Mutex::new(GuardianState::default()));
        state.lock().unwrap().disputes.push(42);
        let (sk, _) = generate_keypair();
        let _ = GUARDIAN_SK.get_or_init(|| sk);
        handle_escalate(&state, 42, Some(vec![1, 2, 3]));
        assert_eq!(state.lock().unwrap().refund_signatures.len(), 1);
    }

    #[test]
    fn discrepancy_without_refund() {
        let _g = test_guard();
        let state = Arc::new(Mutex::new(GuardianState::default()));
        state.lock().unwrap().disputes.push(7);
        handle_escalate(&state, 7, None);
        assert!(state.lock().unwrap().refund_signatures.is_empty());
    }
}
