use std::collections::{HashMap, VecDeque};
#[cfg(any(test, feature = "okcp_relay"))]
use std::fs;
#[cfg(any(test, feature = "okcp_relay"))]
use std::io::{self, ErrorKind};
use std::net::UdpSocket;
#[cfg(any(test, feature = "okcp_relay"))]
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use thiserror::Error;
use tokio::sync::Mutex;
#[cfg(feature = "okcp_relay")]
use tokio::time::{sleep, Duration};

#[cfg(any(test, feature = "okcp_relay"))]
use crate::sim_router::EngineChannel;
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use kaspa_addresses::{Address, Prefix as AddrPrefix, Version as AddrVersion};
use kaspa_consensus_core::{
    network::{NetworkId, NetworkType},
    tx::{TransactionOutpoint, UtxoEntry},
    Hash,
};
use kaspa_rpc_core::api::rpc::RpcApi;
#[cfg(any(test, feature = "okcp_relay"))]
use kaspa_rpc_core::model::block::RpcBlock;
use kaspa_wrpc_client::client::KaspaRpcClient;
#[cfg(any(test, feature = "okcp_relay"))]
use kdapp::engine::EngineMsg;
#[cfg(any(test, feature = "okcp_relay"))]
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
use crate::tlv::{verify_attestation, Attestation, MsgType, TlvMsg, DEMO_HMAC_KEY, SCRIPT_POLICY_VERSION};

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

#[derive(Clone, serde::Serialize, Default)]
pub struct AttestationSummary {
    pub root_hash: [u8; 32],
    pub epoch: u64,
    pub fee_bucket: u64,
    pub count: usize,

    pub by_key: Vec<PubKey>,

    pub last_updated_ts: u64,
}

type AttestCacheInner = HashMap<[u8; 32], Vec<(u64, Attestation)>>;
type AttestCache = Arc<StdMutex<AttestCacheInner>>;

static ATTEST_CACHE: Lazy<AttestCache> = Lazy::new(|| Arc::new(StdMutex::new(HashMap::new())));

#[derive(Debug, Error)]
pub enum AttestationError {
    #[error("bad signature")]
    BadSignature,
}

pub fn ingest_attestation(att: Attestation) -> Result<(), AttestationError> {
    if !verify_attestation(&att) {
        return Err(AttestationError::BadSignature);
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let root = att.root_hash;
    let mut cache = ATTEST_CACHE.lock().expect("attest cache lock");
    let mut remove_root = false;
    {
        let entry = cache.entry(root).or_default();
        if entry.iter().any(|(_, a)| a.attester_pubkey == att.attester_pubkey) {
            // duplicate key, skip
        } else {
            info!(
                "attestation root={} key={} fee_bucket={} cong={}",
                hex::encode(att.root_hash),
                hex::encode(att.attester_pubkey.0.serialize()),
                att.fee_bucket,
                att.congestion_ratio
            );
            entry.push((now, att));
        }
        entry.retain(|(ts, _)| now.saturating_sub(*ts) <= 60);
        if entry.is_empty() {
            remove_root = true;
        }
    }
    if remove_root {
        cache.remove(&root);
    }
    Ok(())
}

pub fn attestation_summaries() -> Vec<AttestationSummary> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let mut out = Vec::new();
    let mut cache = ATTEST_CACHE.lock().expect("attest cache lock");
    cache.retain(|root, list| {
        list.retain(|(ts, _)| now.saturating_sub(*ts) <= 60);
        if list.is_empty() {
            false
        } else {
            let count = list.len();
            let by_key = list.iter().map(|(_, a)| a.attester_pubkey).collect::<Vec<_>>();
            let last_updated_ts = list.iter().map(|(ts, _)| *ts).max().unwrap_or(0);
            let epoch = list.iter().max_by_key(|(ts, _)| *ts).map(|(_, a)| a.epoch).unwrap_or(0);
            let mut fee_counts: HashMap<u64, usize> = HashMap::new();
            for (_, a) in list.iter() {
                *fee_counts.entry(a.fee_bucket).or_insert(0) += 1;
            }
            let fee_bucket = fee_counts.into_iter().max_by_key(|(_, c)| *c).map(|(b, _)| b).unwrap_or(0);
            out.push(AttestationSummary { root_hash: *root, epoch, fee_bucket, count, by_key, last_updated_ts });
            true
        }
    });
    out
}

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
    use kaspa_rpc_core::notify::virtual_chain_changed::{
        VirtualChainChangedNotification, VirtualChainChangedNotificationType,
    };

    let store = RelayCheckpointStore::new(default_state_path());
    let mut last_processed = match store.load() {
        Ok(value) => value,
        Err(err) => {
            warn!("watcher: failed to load relay checkpoint state: {err}");
            None
        }
    };

    if last_processed.is_some() {
        if let Err(err) = startup_rescan(client, program_id, &sender, &store, &mut last_processed).await {
            warn!("watcher: startup rescan failed: {err}");
        }
    }

    let mut backoff = Duration::from_secs(1);
    loop {
        match client.subscribe_virtual_chain_changed().await {
            Ok(mut stream) => {
                backoff = Duration::from_secs(1);
                info!("watcher: subscribed to virtual chain notifications");
                while let Some(VirtualChainChangedNotification { ty, accepted_blocks, .. }) = stream.recv().await {
                    if !matches!(ty, VirtualChainChangedNotificationType::Accepted) {
                        continue;
                    }
                    for block in accepted_blocks {
                        if let Some(batch) = batch_from_notification_block(block, program_id) {
                            process_checkpoint_batches(&sender, &store, &mut last_processed, std::iter::once(batch));
                        }
                    }
                }
                warn!("watcher: virtual chain stream ended; retrying subscription");
            }
            Err(err) => {
                warn!("watcher: failed to subscribe to virtual chain notifications: {err}");
            }
        }
        sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

#[cfg(any(test, feature = "okcp_relay"))]
const DEFAULT_RELAY_STATE_FILE: &str = "okcp_relay_state.hex";

#[cfg(any(test, feature = "okcp_relay"))]
fn default_state_path() -> PathBuf {
    PathBuf::from(DEFAULT_RELAY_STATE_FILE)
}

#[cfg(any(test, feature = "okcp_relay"))]
#[derive(Clone, Debug)]
struct CheckpointBatch {
    accepting_hash: Hash,
    accepting_daa: u64,
    accepting_time: u64,
    checkpoints: Vec<(Hash, Vec<u8>)>,
}

#[cfg(any(test, feature = "okcp_relay"))]
#[derive(Clone, Debug)]
struct RelayCheckpointStore {
    path: PathBuf,
}

#[cfg(any(test, feature = "okcp_relay"))]
impl RelayCheckpointStore {
    fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    fn load(&self) -> io::Result<Option<Hash>> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => {
                let trimmed = contents.trim();
                if trimmed.is_empty() {
                    return Ok(None);
                }
                let bytes = hex::decode(trimmed)
                    .map_err(|err| io::Error::new(ErrorKind::InvalidData, format!("invalid hash encoding: {err}")))?;
                if bytes.len() != 32 {
                    return Err(io::Error::new(ErrorKind::InvalidData, "invalid hash length"));
                }
                let mut array = [0u8; 32];
                array.copy_from_slice(&bytes);
                Ok(Some(Hash::from_bytes(array)))
            }
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn persist(&self, hash: Hash) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(&self.path, hex::encode(hash.as_bytes()))
    }
}

#[cfg(any(test, feature = "okcp_relay"))]
fn batch_from_notification_block(block: kaspa_consensus_core::block::Block, program_id: u64) -> Option<CheckpointBatch> {
    let accepting_hash = block.hash();
    let accepting_daa = block.header.daa_score;
    let accepting_time = block.header.timestamp;
    let mut checkpoints = Vec::new();
    for tx in block.transactions {
        if let Some(payload) = tx.payload() {
            if let Some(rec) = decode_okcp(payload) {
                if rec.program_id == program_id {
                    checkpoints.push((tx.id(), payload.to_vec()));
                }
            }
        }
    }
    build_checkpoint_batch(accepting_hash, accepting_daa, accepting_time, checkpoints)
}

#[cfg(any(test, feature = "okcp_relay"))]
fn batch_from_rpc_block(block: RpcBlock, program_id: u64) -> Option<CheckpointBatch> {
    use std::convert::TryFrom;

    let accepting_hash = block.header.hash;
    let accepting_daa = block.header.daa_score;
    let accepting_time = block.header.timestamp;
    let mut checkpoints = Vec::new();
    for tx in block.transactions {
        if tx.payload.is_empty() {
            continue;
        }
        if let Some(rec) = decode_okcp(&tx.payload) {
            if rec.program_id == program_id {
                let tx_id = if let Some(verbose) = tx.verbose_data.as_ref() {
                    verbose.transaction_id
                } else {
                    match kaspa_consensus_core::tx::Transaction::try_from(tx.clone()) {
                        Ok(consensus_tx) => consensus_tx.id(),
                        Err(err) => {
                            warn!("watcher: failed to convert transaction for accepting block {accepting_hash}: {err}");
                            continue;
                        }
                    }
                };
                checkpoints.push((tx_id, tx.payload.clone()));
            }
        }
    }
    build_checkpoint_batch(accepting_hash, accepting_daa, accepting_time, checkpoints)
}

#[cfg(any(test, feature = "okcp_relay"))]
fn build_checkpoint_batch(
    accepting_hash: Hash,
    accepting_daa: u64,
    accepting_time: u64,
    checkpoints: Vec<(Hash, Vec<u8>)>,
) -> Option<CheckpointBatch> {
    if checkpoints.is_empty() {
        None
    } else {
        Some(CheckpointBatch { accepting_hash, accepting_daa, accepting_time, checkpoints })
    }
}

#[cfg(any(test, feature = "okcp_relay"))]
fn process_checkpoint_batches<I>(
    sender: &EngineChannel,
    store: &RelayCheckpointStore,
    last_processed: &mut Option<Hash>,
    batches: I,
)
where
    I: IntoIterator<Item = CheckpointBatch>,
{
    for batch in batches {
        if batch.checkpoints.is_empty() {
            continue;
        }
        if last_processed.as_ref() == Some(&batch.accepting_hash) {
            continue;
        }
        let CheckpointBatch { accepting_hash, accepting_daa, accepting_time, checkpoints } = batch;
        let associated_txs = checkpoints
            .into_iter()
            .map(|(tx_id, payload)| (tx_id, payload, None::<Vec<TxOutputInfo>>, None))
            .collect::<Vec<_>>();
        let event = EngineMsg::BlkAccepted { accepting_hash, accepting_daa, accepting_time, associated_txs };
        if let Err(err) = sender.send(event) {
            warn!("watcher: failed to forward checkpoint block {accepting_hash}: {err}");
            continue;
        }
        if let Err(err) = store.persist(accepting_hash) {
            warn!("watcher: failed to persist checkpoint state {accepting_hash}: {err}");
            continue;
        }
        *last_processed = Some(accepting_hash);
    }
}

#[cfg(any(test, feature = "okcp_relay"))]
async fn startup_rescan(
    client: &KaspaRpcClient,
    program_id: u64,
    sender: &EngineChannel,
    store: &RelayCheckpointStore,
    last_processed: &mut Option<Hash>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(mut anchor) = *last_processed else { return Ok(()); };

    loop {
        let mut response = client.get_virtual_chain_from_block(anchor, true).await?;
        if response.accepted_transaction_ids.is_empty() {
            break;
        }
        let next_anchor = response
            .accepted_transaction_ids
            .last()
            .map(|entry| entry.accepting_block_hash)
            .unwrap_or(anchor);
        for entry in response.accepted_transaction_ids.drain(..) {
            let accepting_hash = entry.accepting_block_hash;
            if last_processed.as_ref() == Some(&accepting_hash) {
                continue;
            }
            match client.get_block(accepting_hash, true).await {
                Ok(block) => {
                    if let Some(batch) = batch_from_rpc_block(block, program_id) {
                        process_checkpoint_batches(sender, store, last_processed, std::iter::once(batch));
                    }
                }
                Err(err) => {
                    warn!("watcher: failed to fetch block {accepting_hash} during rescan: {err}");
                }
            }
        }
        if next_anchor == anchor {
            break;
        }
        anchor = next_anchor;
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
                script_policy_version: SCRIPT_POLICY_VERSION,
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
            script_policy_version: SCRIPT_POLICY_VERSION,
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
    use std::sync::mpsc::channel;

    use tempfile::TempDir;

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

    fn hash_from_byte(byte: u8) -> Hash {
        let mut data = [0u8; 32];
        data[31] = byte;
        Hash::from_bytes(data)
    }

    fn sample_batch(block_byte: u8, payload_byte: u8) -> CheckpointBatch {
        CheckpointBatch {
            accepting_hash: hash_from_byte(block_byte),
            accepting_daa: block_byte as u64,
            accepting_time: 1_000 + block_byte as u64,
            checkpoints: vec![(hash_from_byte(payload_byte), vec![payload_byte])],
        }
    }

    #[test]
    fn checkpoint_batches_resume_from_persisted_state() {
        let dir = TempDir::new().expect("tempdir");
        let store = RelayCheckpointStore::new(dir.path().join("relay_state"));
        let (tx, rx) = channel();
        let channel = EngineChannel::Local(tx);
        let mut last_processed = None;

        let first = sample_batch(1, 11);
        let second = sample_batch(2, 22);
        let third = sample_batch(3, 33);

        process_checkpoint_batches(&channel, &store, &mut last_processed, vec![first.clone()]);
        assert_eq!(store.load().expect("load state"), Some(first.accepting_hash));
        // Drain the initial event so later assertions focus on the catch-up run.
        rx.recv().expect("initial event");

        let mut last_processed = store.load().expect("reload state");
        process_checkpoint_batches(
            &channel,
            &store,
            &mut last_processed,
            vec![first.clone(), second.clone(), third.clone()],
        );

        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events.len(), 2);

        match &events[0] {
            EngineMsg::BlkAccepted { accepting_hash, associated_txs, .. } => {
                assert_eq!(*accepting_hash, second.accepting_hash);
                assert_eq!(associated_txs.len(), 1);
                assert_eq!(associated_txs[0].1, vec![22]);
            }
            other => panic!("unexpected event: {other:?}"),
        }
        match &events[1] {
            EngineMsg::BlkAccepted { accepting_hash, associated_txs, .. } => {
                assert_eq!(*accepting_hash, third.accepting_hash);
                assert_eq!(associated_txs.len(), 1);
                assert_eq!(associated_txs[0].1, vec![33]);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        assert_eq!(last_processed, Some(third.accepting_hash));
        assert_eq!(store.load().expect("final state"), Some(third.accepting_hash));
    }
}
