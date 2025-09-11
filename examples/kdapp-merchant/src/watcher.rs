use std::collections::VecDeque;
use std::net::UdpSocket;
use std::sync::{Arc, RwLock};
use std::thread;

use once_cell::sync::Lazy;
use tokio::sync::Mutex;

#[cfg(feature = "okcp_relay")]
use crate::sim_router::EngineChannel;
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use kaspa_addresses::{Address, Prefix as AddrPrefix, Version as AddrVersion};
use kaspa_consensus_core::{
    network::{NetworkId, NetworkType},
    tx::{TransactionOutpoint, UtxoEntry},
};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::client::KaspaRpcClient;
#[cfg(feature = "okcp_relay")]
use kdapp::engine::EngineMsg;
#[cfg(feature = "okcp_relay")]
use kdapp::episode::TxOutputInfo;
use kdapp::pki::{to_message, verify_signature, PubKey, Sig};
use kdapp::{
    generator::{PatternType, PrefixType, TransactionGenerator},
    proxy,
};
use log::{info, warn};
use secp256k1::Keypair;
use serde::Serialize;

use crate::server::WatcherRuntimeOverrides;
use crate::tlv::{MsgType, TlvMsg, DEMO_HMAC_KEY};

pub const MIN_FEE: u64 = 5_000;
const CHECKPOINT_PREFIX: PrefixType = u32::from_le_bytes(*b"KMCP");
#[derive(Clone, Serialize)]
pub struct MempoolSnapshot {
    pub est_base_fee: u64,
    pub congestion_ratio: f64,
    pub min_fee: u64,
    pub max_fee: u64,
}

#[derive(Clone, Serialize)]
struct PolicyInfo {
    min: u64,
    max: u64,
    policy: String,
    selected_fee: u64,
    deferred: bool,
}

pub trait FeePolicy: Send + Sync + 'static {
    fn fee_and_deferral(&self, snap: &MempoolSnapshot) -> (u64, bool);
    fn into_kind(self: Box<Self>) -> FeePolicyKind;
}

pub struct StaticFeePolicy {
    pub fee: u64,
}

impl FeePolicy for StaticFeePolicy {
    fn fee_and_deferral(&self, _snap: &MempoolSnapshot) -> (u64, bool) {
        (self.fee, false)
    }
    fn into_kind(self: Box<Self>) -> FeePolicyKind {
        FeePolicyKind::Static(*self)
    }
}

pub struct CongestionAwarePolicy {
    pub min_fee: u64,
    pub max_fee: u64,
    pub defer_threshold: f64,
    pub multiplier: f64,
}

impl FeePolicy for CongestionAwarePolicy {
    fn fee_and_deferral(&self, snap: &MempoolSnapshot) -> (u64, bool) {
        if snap.congestion_ratio >= self.defer_threshold {
            return (snap.min_fee, true);
        }
        let scaled = (snap.est_base_fee as f64 * (1.0 + self.multiplier * snap.congestion_ratio)).ceil() as u64;
        let clamped = scaled.clamp(self.min_fee, self.max_fee);
        (clamped, false)
    }
    fn into_kind(self: Box<Self>) -> FeePolicyKind {
        FeePolicyKind::Congestion(*self)
    }
}

pub enum FeePolicyKind {
    Static(StaticFeePolicy),
    Congestion(CongestionAwarePolicy),
}

impl FeePolicyKind {
    pub fn as_dyn(&self) -> &dyn FeePolicy {
        match self {
            FeePolicyKind::Static(p) => p,
            FeePolicyKind::Congestion(p) => p,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            FeePolicyKind::Static(_) => "static",
            FeePolicyKind::Congestion(_) => "congestion",
        }
    }
}

pub static WATCHER_OVERRIDES: Lazy<Arc<Mutex<WatcherRuntimeOverrides>>> =
    Lazy::new(|| Arc::new(Mutex::new(WatcherRuntimeOverrides::default())));

pub static MEMPOOL_METRICS: Lazy<RwLock<Option<MempoolSnapshot>>> = Lazy::new(|| RwLock::new(None));
static POLICY_INFO: Lazy<RwLock<PolicyInfo>> = Lazy::new(|| {
    RwLock::new(PolicyInfo { min: MIN_FEE, max: MIN_FEE, policy: "static".to_string(), selected_fee: MIN_FEE, deferred: false })
});

pub fn get_metrics() -> Option<MempoolSnapshot> {
    MEMPOOL_METRICS.read().expect("metrics lock").clone()
}

fn store_metrics(snap: MempoolSnapshot) {
    *MEMPOOL_METRICS.write().expect("metrics lock") = Some(snap);
}

#[derive(Serialize)]
struct WatcherMetrics {
    est_base_fee: u64,
    congestion_ratio: f64,
    min: u64,
    max: u64,
    policy: String,
    selected_fee: u64,
    deferred: bool,
}

async fn get_mempool_metrics() -> Result<Json<WatcherMetrics>, StatusCode> {
    if let Some(snap) = get_metrics() {
        let p = POLICY_INFO.read().expect("policy lock").clone();
        Ok(Json(WatcherMetrics {
            est_base_fee: snap.est_base_fee,
            congestion_ratio: snap.congestion_ratio,
            min: p.min,
            max: p.max,
            policy: p.policy,
            selected_fee: p.selected_fee,
            deferred: p.deferred,
        }))
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

fn pattern() -> PatternType {
    [(0u8, 0u8); 10]
}

fn encode_okcp(episode_id: u64, seq: u64, root: [u8; 32]) -> Vec<u8> {
    let mut rec = Vec::with_capacity(4 + 1 + 8 + 8 + 32);
    rec.extend_from_slice(b"OKCP");
    rec.push(1u8);
    rec.extend_from_slice(&episode_id.to_le_bytes());
    rec.extend_from_slice(&seq.to_le_bytes());
    rec.extend_from_slice(&root);
    rec
}

#[derive(Debug, PartialEq, Eq)]
pub struct OkcpRecord {
    pub program_id: u64,
    pub seq: u64,
    pub root: [u8; 32],
}

#[cfg(any(test, feature = "okcp_relay"))]
pub fn decode_okcp(bytes: &[u8]) -> Option<OkcpRecord> {
    // Format: b"OKCP" (4) | version (1) | program_id (u64 LE) | seq (u64 LE) | root ([u8;32])
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
    let root_end = seq_end + 32;
    let program_id = u64::from_le_bytes(bytes[pid_start..pid_end].try_into().ok()?);
    let seq = u64::from_le_bytes(bytes[pid_end..seq_end].try_into().ok()?);
    let mut root = [0u8; 32];
    root.copy_from_slice(&bytes[seq_end..root_end]);
    Some(OkcpRecord { program_id, seq, root })
}

#[cfg(feature = "okcp_relay")]
pub async fn relay_checkpoints(
    client: &KaspaRpcClient,
    program_id: u64,
    sender: EngineChannel,
) -> Result<(), Box<dyn std::error::Error>> {
    use kaspa_rpc_core::notify::virtual_chain_changed::{VirtualChainChangedNotification, VirtualChainChangedNotificationType};
    let mut stream = client.subscribe_virtual_chain_changed().await?;
    while let Some(VirtualChainChangedNotification { ty, accepted_blocks, .. }) = stream.recv().await {
        if !matches!(ty, VirtualChainChangedNotificationType::Accepted) {
            continue;
        }
        for block in accepted_blocks {
            let accepting_hash = block.hash();
            let accepting_daa = block.header.daa_score;
            let accepting_time = block.header.timestamp;
            for tx in block.transactions {
                if let Some(payload) = tx.payload() {
                    if let Some(rec) = decode_okcp(payload) {
                        if rec.program_id == program_id {
                            let tx_id = tx.id();
                            let event = EngineMsg::BlkAccepted {
                                accepting_hash,
                                accepting_daa,
                                accepting_time,
                                associated_txs: vec![(tx_id, payload.to_vec(), None::<Vec<TxOutputInfo>>)],
                            };
                            let _ = sender.send(event);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn fetch_mempool_snapshot(client: &KaspaRpcClient) -> Result<MempoolSnapshot, String> {
    // Base fee: derive from fee estimate (sompi), with a conservative mass assumption and MIN_FEE floor
    let estimate = client.get_fee_estimate().await.map_err(|e| e.to_string())?;
    let feerate = estimate.normal_buckets.first().map(|b| b.feerate).unwrap_or(estimate.priority_bucket.feerate);
    // Approximate small tx mass (1 in / 1 out with payload); adjust as needed
    let approx_mass: f64 = 200.0;
    let mut est_base_fee = (feerate * approx_mass).ceil() as u64;
    if est_base_fee < MIN_FEE {
        est_base_fee = MIN_FEE;
    }

    // Congestion: use consensus metrics' network_mempool_size as a simple heuristic
    let congestion = match client.get_metrics(false, false, false, true, false, false).await {
        Ok(m) => m.consensus_metrics.map(|cm| (cm.network_mempool_size as f64) / 10_000.0).unwrap_or(0.0),
        Err(_) => 0.0,
    };

    Ok(MempoolSnapshot { est_base_fee, congestion_ratio: congestion, min_fee: MIN_FEE, max_fee: MIN_FEE })
}

pub async fn update_metrics(client: &KaspaRpcClient) -> Result<MempoolSnapshot, String> {
    let snap = fetch_mempool_snapshot(client).await?;
    store_metrics(snap.clone());
    Ok(snap)
}

fn apply_overrides(
    base_min_fee: u64,
    base_max_fee: u64,
    base_defer_threshold: f64,
    overrides: &WatcherRuntimeOverrides,
) -> (u64, u64, f64) {
    let max_fee = overrides.max_fee.unwrap_or(base_max_fee);
    let defer_threshold = overrides.congestion_threshold.unwrap_or(base_defer_threshold);
    (base_min_fee, max_fee, defer_threshold)
}

async fn submit_checkpoint_tx(
    episode_id: u64,
    seq: u64,
    root: [u8; 32],
    sk_hex: &str,
    mainnet: bool,
    wrpc_url: Option<String>,
    fee: u64,
) -> Result<(), String> {
    let mut sk_bytes = [0u8; 32];
    faster_hex::hex_decode(sk_hex.trim().as_bytes(), &mut sk_bytes).map_err(|_| "invalid private key hex".to_string())?;
    let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &sk_bytes).map_err(|_| "invalid sk".to_string())?;
    let network = if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
    let addr_prefix = if mainnet { AddrPrefix::Mainnet } else { AddrPrefix::Testnet };
    let addr = Address::new(addr_prefix, AddrVersion::PubKey, &keypair.x_only_public_key().0.serialize());

    let kaspad = proxy::connect_client(network, wrpc_url).await.map_err(|e| e.to_string())?;
    let utxos = kaspad
        .get_utxos_by_addresses(vec![addr.clone()])
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|u| (TransactionOutpoint::from(u.outpoint), UtxoEntry::from(u.utxo_entry)))
        .collect::<Vec<_>>();
    if utxos.is_empty() {
        return Err(format!("no UTXOs for {addr}"));
    }
    let (op, entry) = utxos.iter().max_by_key(|(_, e)| e.amount).cloned().unwrap();
    if entry.amount <= fee {
        return Err(format!("selected UTXO too small: {}", entry.amount));
    }

    let payload = encode_okcp(episode_id, seq, root);
    let gen = TransactionGenerator::new(keypair, pattern(), CHECKPOINT_PREFIX);
    let send = entry.amount - fee;
    let tx = gen.build_transaction(&[(op, entry)], send, 1, &addr, payload);
    submit_tx_retry(&kaspad, &tx, 3).await
}

async fn submit_tx_retry(kaspad: &KaspaRpcClient, tx: &kaspa_consensus_core::tx::Transaction, attempts: usize) -> Result<(), String> {
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
    policy: Box<dyn FeePolicy>,
    http_port: Option<u16>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut policy = policy.into_kind();
    {
        let mut info = POLICY_INFO.write().expect("policy lock");
        match &policy {
            FeePolicyKind::Static(p) => {
                info.min = p.fee;
                info.max = p.fee;
            }
            FeePolicyKind::Congestion(p) => {
                info.min = p.min_fee;
                info.max = p.max_fee;
            }
        }
        info.policy = policy.name().to_string();
    }
    if let Some(port) = http_port {
        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            rt.block_on(async move {
                let addr = format!("0.0.0.0:{port}");
                if let Ok(listener) = tokio::net::TcpListener::bind(&addr).await {
                    info!("watcher http listening on {addr}");
                    let app = Router::new().route("/mempool", get(get_mempool_metrics));
                    let _ = axum::serve(listener, app).await;
                }
            });
        });
    }

    let sock = UdpSocket::bind(bind)?;
    info!("watcher listening on {bind}");
    let rt = tokio::runtime::Runtime::new()?;
    let mut buf = [0u8; 1024];
    let mut pending: VecDeque<(u64, u64, [u8; 32])> = VecDeque::new();
    loop {
        let (n, src) = sock.recv_from(&mut buf)?;
        let Some(msg) = TlvMsg::decode(&buf[..n]) else {
            warn!("watcher: invalid TLV from {src}");
            continue;
        };
        // Respond to handshake for compatibility with client_sender retries
        if msg.msg_type == MsgType::Handshake as u8 {
            let mut ack = TlvMsg {
                version: msg.version,
                msg_type: MsgType::Ack as u8,
                episode_id: msg.episode_id,
                seq: msg.seq,
                state_hash: msg.state_hash,
                payload: vec![],
                auth: [0u8; 32],
            };
            ack.sign(DEMO_HMAC_KEY);
            let _ = sock.send_to(&ack.encode(), src);
            continue;
        }
        if msg.msg_type == MsgType::Refund as u8 {
            if !msg.verify(DEMO_HMAC_KEY) {
                warn!("watcher: invalid refund from {src}");
                continue;
            }
            if let Ok((tx, sig, gpk)) = borsh::from_slice::<(Vec<u8>, Sig, PubKey)>(&msg.payload) {
                let m = to_message(&tx);
                if verify_signature(&gpk, &m, &sig) {
                    info!("refund verified for ep={} seq={}", msg.episode_id, msg.seq);
                } else {
                    warn!("watcher: invalid guardian signature on refund");
                }
            }
            continue;
        }
        if msg.msg_type != MsgType::Checkpoint as u8 || !msg.verify(DEMO_HMAC_KEY) {
            warn!("watcher: ignored msg from {src}");
            continue;
        }
        // Acknowledge the checkpoint receipt to the sender
        let mut ack = TlvMsg {
            version: msg.version,
            msg_type: MsgType::Ack as u8,
            episode_id: msg.episode_id,
            seq: msg.seq,
            state_hash: msg.state_hash,
            payload: vec![],
            auth: [0u8; 32],
        };
        ack.sign(DEMO_HMAC_KEY);
        let _ = sock.send_to(&ack.encode(), src);
        let root = msg.state_hash;
        let ep = msg.episode_id;
        let seq = msg.seq;
        info!("checkpoint received: ep={ep} seq={seq}");
        pending.push_back((ep, seq, root));
        let key = kaspa_private_key.clone();
        let url = wrpc_url.clone();
        let base_snap = match rt.block_on({
            let url_clone = url.clone();
            async move {
                let network =
                    if mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
                let client = proxy::connect_client(network, url_clone).await.map_err(|e| e.to_string())?;
                fetch_mempool_snapshot(&client).await
            }
        }) {
            Ok(v) => v,
            Err(e) => {
                warn!("fee metrics unavailable: {e}");
                MempoolSnapshot { est_base_fee: MIN_FEE, congestion_ratio: 0.0, min_fee: MIN_FEE, max_fee: MIN_FEE }
            }
        };
        info!("mempool congestion: {:.2}", base_snap.congestion_ratio);

        let (base_min, base_max, base_thr) = match &policy {
            FeePolicyKind::Static(p) => (p.fee, p.fee, 1.0),
            FeePolicyKind::Congestion(p) => (p.min_fee, p.max_fee, p.defer_threshold),
        };
        let (min_fee_now, max_fee_now, defer_thr_now) = {
            let overrides = WATCHER_OVERRIDES.blocking_lock();
            apply_overrides(base_min, base_max, base_thr, &overrides)
        };
        if let FeePolicyKind::Congestion(ref mut p) = policy {
            p.min_fee = min_fee_now;
            p.max_fee = max_fee_now;
            p.defer_threshold = defer_thr_now;
        }
        let snapshot = MempoolSnapshot {
            est_base_fee: base_snap.est_base_fee,
            congestion_ratio: base_snap.congestion_ratio,
            min_fee: min_fee_now,
            max_fee: max_fee_now,
        };
        store_metrics(snapshot.clone());
        let (fee, defer) = policy.as_dyn().fee_and_deferral(&snapshot);
        {
            let mut info = POLICY_INFO.write().expect("policy lock");
            info.min = min_fee_now;
            info.max = max_fee_now;
            info.policy = policy.name().to_string();
            info.selected_fee = fee;
            info.deferred = defer;
        }
        if defer {
            warn!("congestion high; deferring anchor (ratio {:.2})", snapshot.congestion_ratio);
            continue;
        }
        info!("processing {} queued checkpoints", pending.len());
        while let Some((ep, seq, root)) = pending.pop_front() {
            if let Err(e) = rt.block_on(submit_checkpoint_tx(ep, seq, root, &key, mainnet, url.clone(), fee)) {
                warn!("anchor failed: {e}");
            } else {
                info!("anchor submitted for ep={ep} seq={seq}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn okcp_roundtrip() {
        let root = [3u8; 32];
        let data = encode_okcp(42, 7, root);
        let rec = decode_okcp(&data).expect("decode okcp");
        assert_eq!(rec.program_id, 42);
        assert_eq!(rec.seq, 7);
        assert_eq!(rec.root, root);
    }

    #[test]
    fn static_policy_no_defer() {
        let policy = StaticFeePolicy { fee: 5 };
        let snap = MempoolSnapshot { est_base_fee: 100, congestion_ratio: 0.9, min_fee: 1, max_fee: 10 };
        let (fee, defer) = policy.fee_and_deferral(&snap);
        assert_eq!(fee, 5);
        assert!(!defer);
    }

    #[test]
    fn congestion_policy_defers_on_threshold() {
        let policy = CongestionAwarePolicy { min_fee: 1, max_fee: 10_000, defer_threshold: 0.5, multiplier: 1.0 };
        let snap = MempoolSnapshot { est_base_fee: 100, congestion_ratio: 0.6, min_fee: 1, max_fee: 10_000 };
        let (fee, defer) = policy.fee_and_deferral(&snap);
        assert!(defer);
        assert_eq!(fee, 1);
    }

    #[test]
    fn congestion_policy_scales_and_clamps() {
        let policy = CongestionAwarePolicy { min_fee: 1, max_fee: 10, defer_threshold: 0.9, multiplier: 1.0 };
        let snap = MempoolSnapshot { est_base_fee: 6_000, congestion_ratio: 0.2, min_fee: 1, max_fee: 10 };
        let (fee, defer) = policy.fee_and_deferral(&snap);
        assert!(!defer);
        assert!((1..=10).contains(&fee));
    }
}
