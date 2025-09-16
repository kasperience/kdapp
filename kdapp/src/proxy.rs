//! Contains methods for creating a Kaspa wrpc client as well as listener logic for following
//! accepted txs by id pattern and prefix and sending them to corresponding engines.

use kaspa_consensus_core::{network::NetworkId, Hash};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::RpcNetworkType;
use kaspa_wrpc_client::client::ConnectOptions;
use kaspa_wrpc_client::error::Error;
use kaspa_wrpc_client::prelude::*;
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use lru::LruCache;

use log::{debug, info, warn};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::num::NonZeroUsize;
use std::time::Duration;
use tokio::time::{sleep_until, Instant};

use crate::episode::TxOutputInfo;
use crate::generator::{PatternType, PrefixType};
use crate::{
    engine::EngineMsg as Msg,
    generator::{check_pattern, Payload},
};

pub fn connect_options() -> ConnectOptions {
    ConnectOptions {
        block_async_connect: true,
        strategy: ConnectStrategy::Fallback,
        url: None,
        connect_timeout: Some(Duration::from_secs(5)),
        // Enable periodic reconnect attempts when the socket drops
        retry_interval: Some(Duration::from_secs(2)),
    }
}

// Copied from https://github.com/supertypo/simply-kaspa-indexer/blob/main/kaspad/src/pool/manager.rs
pub async fn connect_client(network_id: NetworkId, rpc_url: Option<String>) -> Result<KaspaRpcClient, Error> {
    let url = if let Some(url) = &rpc_url { url } else { &Resolver::default().get_url(WrpcEncoding::Borsh, network_id).await? };

    debug!("Connecting to Kaspad {url}");
    let client = KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, Some(url), None, Some(network_id), None)?;
    client.connect(Some(connect_options())).await.map_err(|e| {
        warn!("Kaspad connection failed: {e}");
        e
    })?;

    let server_info = client.get_server_info().await?;
    let connected_network = format!(
        "{}{}",
        server_info.network_id.network_type,
        server_info.network_id.suffix.map(|s| format!("-{s}")).unwrap_or_default()
    );
    info!("Connected to Kaspad {url}, version: {}, network: {connected_network}", server_info.server_version);

    if network_id != server_info.network_id {
        panic!("Network mismatch, expected '{network_id}', actual '{connected_network}'");
    } else if !server_info.is_synced
        || server_info.network_id.network_type == RpcNetworkType::Mainnet && server_info.virtual_daa_score < 107107107
    {
        let err_msg = format!("Kaspad {} is NOT synced", server_info.server_version);
        warn!("{err_msg}");
        Err(Error::Custom(err_msg))
    } else {
        Ok(client)
    }
}

pub type EngineMap = HashMap<PrefixType, (PatternType, Sender<Msg>)>;

#[derive(Clone, Debug)]
pub struct ProxyCacheConfig {
    tx_output_cache_capacity: Option<usize>,
}

impl ProxyCacheConfig {
    pub const DEFAULT_TX_OUTPUT_CACHE_CAPACITY: usize = 256;

    pub fn with_capacity(capacity: usize) -> Self {
        let normalized = if capacity == 0 { None } else { Some(capacity) };
        Self { tx_output_cache_capacity: normalized }
    }

    pub fn disabled() -> Self {
        Self { tx_output_cache_capacity: None }
    }

    pub fn tx_output_cache_capacity(&self) -> Option<usize> {
        self.tx_output_cache_capacity
    }
}

impl Default for ProxyCacheConfig {
    fn default() -> Self {
        Self::with_capacity(Self::DEFAULT_TX_OUTPUT_CACHE_CAPACITY)
    }
}

#[derive(Debug)]
struct TxOutputCache {
    inner: Option<LruCache<(Hash, u32), TxOutputInfo>>,
}

impl TxOutputCache {
    fn new(capacity: Option<usize>) -> Self {
        let inner = capacity.and_then(NonZeroUsize::new).map(LruCache::new);
        Self { inner }
    }

    fn get_or_insert_with<F>(&mut self, txid: &Hash, index: u32, producer: F) -> TxOutputInfo
    where
        F: FnOnce() -> TxOutputInfo,
    {
        if let Some(cache) = self.inner.as_mut() {
            let key = (*txid, index);
            if let Some(existing) = cache.get(&key) {
                return existing.clone();
            }
            let value = producer();
            cache.put(key, value.clone());
            value
        } else {
            producer()
        }
    }
}

pub async fn run_listener(kaspad: KaspaRpcClient, engines: EngineMap, exit_signal: Arc<AtomicBool>) {
    run_listener_with_config(kaspad, engines, exit_signal, ProxyCacheConfig::default()).await;
}

pub async fn run_listener_with_config(
    kaspad: KaspaRpcClient,
    engines: EngineMap,
    exit_signal: Arc<AtomicBool>,
    cache_config: ProxyCacheConfig,
) {
    let mut tx_output_cache = TxOutputCache::new(cache_config.tx_output_cache_capacity());

    let info = match kaspad.get_block_dag_info().await {
        Ok(info) => info,
        Err(e) => {
            warn!("Failed to get block DAG info: {e}. Attempting reconnect...");
            // Try to (re)connect and fetch again
            if let Err(err) = kaspad.connect(Some(connect_options())).await {
                warn!("Reconnect failed: {err}");
                return;
            }
            match kaspad.get_block_dag_info().await {
                Ok(info) => info,
                Err(e2) => {
                    warn!("Failed to get block DAG info after reconnect: {e2}");
                    return;
                }
            }
        }
    };
    let mut sink = info.sink;
    let mut now = Instant::now();
    info!("Sink: {sink}");
    loop {
        if exit_signal.load(Ordering::Relaxed) {
            info!("Exiting...");
            break;
        }
        sleep_until(now + Duration::from_secs(1)).await;
        now = Instant::now();

        let vcb = match kaspad.get_virtual_chain_from_block(sink, true).await {
            Ok(vcb) => vcb,
            Err(e) => {
                warn!("Failed to get virtual chain from block: {e}. Reconnecting...");
                // Attempt a reconnect and reset sink
                if let Err(err) = kaspad.connect(Some(connect_options())).await {
                    warn!("Reconnect failed: {err}");
                    continue;
                }
                match kaspad.get_block_dag_info().await {
                    Ok(info) => {
                        sink = info.sink;
                        info!("Sink: {sink}");
                    }
                    Err(e2) => {
                        warn!("Failed to refresh DAG info after reconnect: {e2}");
                    }
                }
                continue;
            }
        };

        debug!("vspc: {}, {}", vcb.removed_chain_block_hashes.len(), vcb.accepted_transaction_ids.len());

        if let Some(new_sink) = vcb.accepted_transaction_ids.last().map(|ncb| ncb.accepting_block_hash) {
            sink = new_sink;
        } else {
            // No new added chain blocks. This means no removed chain blocks as well so we can continue
            continue;
        }

        for rcb in vcb.removed_chain_block_hashes {
            for (_, sender) in engines.values() {
                let msg = Msg::BlkReverted { accepting_hash: rcb };
                if let Err(e) = sender.send(msg) {
                    warn!("Failed to send block reverted message to engine: {e}");
                }
            }
        }

        // Iterate new chain blocks
        for ncb in vcb.accepted_transaction_ids {
            let accepting_hash = ncb.accepting_block_hash;

            // Required txs kept in original acceptance order. Skip the first which is always a coinbase tx
            let required_txs: Vec<Hash> = ncb
                .accepted_transaction_ids
                .iter()
                .copied()
                .skip(1)
                .filter(|&id| engines.values().any(|(pattern, _)| check_pattern(id, pattern)))
                .collect();

            // Track the required payloads and outputs
            let mut required_payloads: HashMap<Hash, Option<Vec<u8>>> = required_txs.iter().map(|&id| (id, None)).collect();
            let mut required_outputs: HashMap<Hash, Option<Vec<TxOutputInfo>>> = required_txs.iter().map(|&id| (id, None)).collect();
            let mut required_num = required_payloads.len();

            if required_num == 0 {
                continue;
            }

            let accepting_block = match kaspad.get_block(accepting_hash, false).await {
                Ok(block) => block,
                Err(e) => {
                    warn!("Failed to get accepting block {accepting_hash}: {e}. Skipping...");
                    continue;
                }
            };
            let verbose = match accepting_block.verbose_data {
                Some(verbose) => verbose,
                None => {
                    warn!("Accepting block {accepting_hash} has no verbose data. Skipping...");
                    continue;
                }
            };
            // Be resilient to occasional RPC anomalies; avoid panicking on structure assumptions
            if verbose.merge_set_blues_hashes.is_empty() {
                warn!("Accepting block {accepting_hash} has empty merge set blues; skipping structural assertion");
            } else if verbose.selected_parent_hash != verbose.merge_set_blues_hashes[0] {
                warn!(
                    "Selected parent does not match first mergeset blue (accepting block {}): sp={}, first_blue={}",
                    accepting_hash, verbose.selected_parent_hash, verbose.merge_set_blues_hashes[0]
                );
            }
            debug!(
                "accepting block: {}, selected parent: {}, mergeset len: {}",
                accepting_hash,
                verbose.selected_parent_hash,
                verbose.merge_set_blues_hashes.len() + verbose.merge_set_reds_hashes.len()
            );

            // Iterate over merged blocks until finding all accepted and required txs (the mergeset is guaranteed to contain these txs)
            'outer: for merged_hash in verbose.merge_set_blues_hashes.into_iter().chain(verbose.merge_set_reds_hashes) {
                let merged_block = match kaspad.get_block(merged_hash, true).await {
                    Ok(block) => block,
                    Err(e) => {
                        warn!("Failed to get merged block {merged_hash}: {e}. Skipping...");
                        continue;
                    }
                };
                for tx in merged_block.transactions.into_iter().skip(1) {
                    if let Some(tx_verbose) = tx.verbose_data {
                        let tx_id = tx_verbose.transaction_id;
                        if let Some(required_payload) = required_payloads.get_mut(&tx_id) {
                            if required_payload.is_none() {
                                required_payload.replace(tx.payload);
                                // Collect outputs summary for this transaction
                                if let Some(outputs_slot) = required_outputs.get_mut(&tx_id) {
                                    let mut outputs_info = Vec::new();
                                    for (index, out) in tx.outputs.into_iter().enumerate() {
                                        let info = tx_output_cache.get_or_insert_with(&tx_id, index as u32, move || {
                                            #[cfg(feature = "tx-script-bytes")]
                                            let script_bytes = Some(out.script_public_key.script().to_vec());
                                            #[cfg(not(feature = "tx-script-bytes"))]
                                            let script_bytes = None;
                                            TxOutputInfo {
                                                value: out.value,
                                                script_version: out.script_public_key.version,
                                                script_bytes,
                                            }
                                        });
                                        outputs_info.push(info);
                                    }
                                    outputs_slot.replace(outputs_info);
                                }
                                required_num -= 1;
                                if required_num == 0 {
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
            if required_num != 0 {
                warn!(
                    "kaspad returned inconsistent mergeset ({required_num} remaining required txs not found) for accepting block {accepting_hash}. Continuing...",
                );
                // Skip dispatching for this accepting block to avoid partial delivery
                continue;
            }
            // info!("Tx payloads: {:?}", required_payloads);

            let mut consumed_txs = 0;
            // Iterate over all engines and look for id pattern + prefix
            for (&prefix, (pattern, sender)) in engines.iter() {
                // Collect and strip payloads in the correct order (as maintained by required_txs)
                let associated_txs: Vec<_> = required_txs
                    .iter()
                    .filter_map(|&id| {
                        // First, check the pattern
                        if !check_pattern(id, pattern) {
                            return None;
                        }
                        match required_payloads.entry(id) {
                            Entry::Occupied(entry) => {
                                // The prefix is unique per engine, so once we find a match we can consume the entry
                                if let Some(payload_ref) = entry.get().as_ref() {
                                    if Payload::check_header(payload_ref, prefix) {
                                        if let Some(payload) = entry.remove() {
                                            consumed_txs += 1;
                                            // Also fetch corresponding outputs
                                            let outputs = required_outputs.remove(&id).and_then(|o| o);
                                            return Some((id, Payload::strip_header(payload), outputs));
                                        }
                                    }
                                }
                            }
                            Entry::Vacant(_) => {}
                        }
                        None
                    })
                    .collect();
                for (tx_id, _payload, _outs) in associated_txs.iter() {
                    info!("received episode tx: {tx_id}");
                }
                if !associated_txs.is_empty() {
                    // Normalize header timestamp (ms) to seconds for engine metadata
                    let accepting_time_secs = accepting_block.header.timestamp / 1000;
                    let msg = Msg::BlkAccepted {
                        accepting_hash,
                        accepting_daa: accepting_block.header.daa_score,
                        accepting_time: accepting_time_secs,
                        associated_txs,
                    };
                    if let Err(e) = sender.send(msg) {
                        warn!("Failed to send block accepted message to engine: {e}");
                    }
                }
                if consumed_txs == required_txs.len() {
                    // No need to check additional engines
                    break;
                }
            }
        }
    }

    for (_, sender) in engines.values() {
        if let Err(e) = sender.send(Msg::Exit) {
            warn!("Failed to send exit message to engine: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn hash_from_byte(byte: u8) -> Hash {
        Hash::from_bytes([byte; 32])
    }

    #[test]
    fn tx_output_cache_hits_do_not_invoke_producer() {
        let mut cache = TxOutputCache::new(ProxyCacheConfig::with_capacity(4).tx_output_cache_capacity());
        let txid = hash_from_byte(1);
        let calls = AtomicUsize::new(0);

        let first = cache.get_or_insert_with(&txid, 0, || {
            calls.fetch_add(1, Ordering::SeqCst);
            TxOutputInfo { value: 42, script_version: 0, script_bytes: None }
        });

        let second = cache.get_or_insert_with(&txid, 0, || {
            calls.fetch_add(1, Ordering::SeqCst);
            TxOutputInfo { value: 42, script_version: 0, script_bytes: None }
        });

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(first, second);
    }

    #[test]
    fn disabled_cache_invokes_producer_each_time() {
        let mut cache = TxOutputCache::new(ProxyCacheConfig::disabled().tx_output_cache_capacity());
        let txid = hash_from_byte(2);
        let calls = AtomicUsize::new(0);

        let _ = cache.get_or_insert_with(&txid, 1, || {
            calls.fetch_add(1, Ordering::SeqCst);
            TxOutputInfo { value: 7, script_version: 0, script_bytes: None }
        });

        let _ = cache.get_or_insert_with(&txid, 1, || {
            calls.fetch_add(1, Ordering::SeqCst);
            TxOutputInfo { value: 7, script_version: 0, script_bytes: None }
        });

        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
