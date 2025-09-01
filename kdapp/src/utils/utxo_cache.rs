use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use kaspa_addresses::Address;
use kaspa_rpc_core::model::address::RpcUtxosByAddressesEntry;
use kaspa_wrpc_client::prelude::RpcApi;
use kaspa_wrpc_client::KaspaRpcClient;

/// A short-lived per-address UTXO cache to reduce bursty RPC calls.
type UtxoCacheEntries = HashMap<String, (Instant, Vec<RpcUtxosByAddressesEntry>)>;

#[derive(Clone)]
pub struct UtxoCache {
    ttl_ms: u64,
    inner: Arc<Mutex<UtxoCacheEntries>>,
}

impl UtxoCache {
    /// Create a cache with the given TTL in milliseconds (clamped to [100, 2000]).
    pub fn new(ttl_ms: u64) -> Arc<Self> {
        let ttl = ttl_ms.clamp(100, 2000);
        Arc::new(Self { ttl_ms: ttl, inner: Arc::new(Mutex::new(HashMap::new())) })
    }

    /// Get UTXOs for a single address, using cache if still fresh.
    pub async fn get(
        &self,
        client: &KaspaRpcClient,
        addr: &Address,
    ) -> Result<Vec<RpcUtxosByAddressesEntry>, kaspa_wrpc_client::error::Error> {
        let now = Instant::now();
        let key = addr.to_string();

        if let Ok(cache) = self.inner.lock() {
            if let Some((t, cached)) = cache.get(&key) {
                if now.duration_since(*t).as_millis() < self.ttl_ms as u128 {
                    return Ok(cached.clone());
                }
            }
        }

        // Fetch fresh without holding lock
        let fresh = client.get_utxos_by_addresses(vec![addr.clone()]).await?;
        if let Ok(mut cache) = self.inner.lock() {
            cache.insert(key, (now, fresh.clone()));
        }
        Ok(fresh)
    }

    /// Batch version that returns a map Address->Vec<Entry>.
    pub async fn get_many(
        &self,
        client: &KaspaRpcClient,
        addrs: &[Address],
    ) -> Result<HashMap<Address, Vec<RpcUtxosByAddressesEntry>>, kaspa_wrpc_client::error::Error> {
        // For simplicity, call get() per address; callers usually have few addrs.
        let mut out = HashMap::new();
        for addr in addrs.iter() {
            let v = self.get(client, addr).await?;
            out.insert(addr.clone(), v);
        }
        Ok(out)
    }

    /// Invalidate all cache entries for a given address.
    pub fn invalidate_address(&self, addr: &Address) {
        if let Ok(mut cache) = self.inner.lock() {
            cache.remove(&addr.to_string());
        }
    }

    /// Invalidate any entry containing a specific outpoint (best-effort linear scan).
    pub fn invalidate_outpoint(&self, txid: &str, index: u32) {
        if let Ok(mut cache) = self.inner.lock() {
            let mut to_invalidate: Vec<String> = Vec::new();
            for (k, (_t, entries)) in cache.iter() {
                let contains = entries.iter().any(|e| e.outpoint.transaction_id.to_string() == txid && e.outpoint.index == index);
                if contains {
                    to_invalidate.push(k.clone());
                }
            }
            for k in to_invalidate {
                cache.remove(&k);
            }
        }
    }

    /// Adjust TTL at runtime.
    pub fn set_ttl_ms(&mut self, ttl_ms: u64) {
        self.ttl_ms = ttl_ms.clamp(100, 2000);
    }
    /// Read current TTL.
    pub fn ttl_ms(&self) -> u64 {
        self.ttl_ms
    }
}
