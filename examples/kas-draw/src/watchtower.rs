#![allow(dead_code, clippy::type_complexity)]
#[derive(Clone, Debug)]
pub struct Outpoint {
    pub txid: [u8; 32],
    pub vout: u32,
}

#[derive(Clone, Debug)]
pub struct StateBundle {
    pub funding_outpoint: Outpoint,       // channel anchor
    pub episode_id: u64,                  // kdapp episode id
    pub state_num: u64,                   // monotonically increasing
    pub state_hash: [u8; 32],             // hash of episode state/commit
    pub revocation_secret_prev: [u8; 32], // penalty for stale close
    pub penalty_hint: Vec<u8>,            // compact template or script hint
    pub csv_secs: u32,                    // tower reaction window
}

#[derive(Clone, Debug)]
pub struct UtxoWatch {
    pub outpoint: Outpoint,   // locked entry or winner-claim UTXO
    pub expires_at_unix: u64, // alert/act after this
    pub note: &'static str,   // "locked_entry" | "winner_claim" | etc.
}

#[allow(async_fn_in_trait)]
pub trait WatchtowerClient {
    async fn register_channel(&self, funding_outpoint: Outpoint, episode_id: u64, csv_secs: u32) -> anyhow::Result<()>;
    async fn submit_state(&self, bundle: StateBundle) -> anyhow::Result<()>;
    async fn watch_utxo(&self, watch: UtxoWatch) -> anyhow::Result<()>;
}

pub struct NoopTower;

impl WatchtowerClient for NoopTower {
    async fn register_channel(&self, _funding_outpoint: Outpoint, _episode_id: u64, _csv_secs: u32) -> anyhow::Result<()> {
        Ok(())
    }
    async fn submit_state(&self, _bundle: StateBundle) -> anyhow::Result<()> {
        Ok(())
    }
    async fn watch_utxo(&self, _watch: UtxoWatch) -> anyhow::Result<()> {
        Ok(())
    }
}

use log::{info, warn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A simple in-process tower simulator for off-chain testing.
#[derive(Default, Clone)]
pub struct SimTower {
    inner: Arc<Mutex<HashMap<u64, (u64, [u8; 32])>>>,
}

impl SimTower {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Record a new state and log: OK if strictly newer; warn on stale/out-of-order.
    pub fn on_state(&self, episode_id: u64, state_num: u64, state_hash: [u8; 32]) {
        let mut m = self.inner.lock().unwrap();
        match m.get(&episode_id) {
            Some((prev_num, prev_hash)) => {
                if state_num <= *prev_num {
                    warn!("tower: stale/out-of-order state for ep {episode_id} (got {state_num}, last={prev_num})");
                } else {
                    info!(
                        "tower: ep {} state advanced {} -> {} (prev_hash={:02x?})",
                        episode_id,
                        prev_num,
                        state_num,
                        &prev_hash[..4]
                    );
                    m.insert(episode_id, (state_num, state_hash));
                }
            }
            None => {
                info!("tower: ep {episode_id} first state {state_num}");
                m.insert(episode_id, (state_num, state_hash));
            }
        }
    }

    pub fn finalize(&self, episode_id: u64, last_state_hash: [u8; 32]) {
        let mut m = self.inner.lock().unwrap();
        let last = m.remove(&episode_id);
        match last {
            Some((n, _)) => info!("tower: Finalized ep {} at state {} (hash {:02x?})", episode_id, n, &last_state_hash[..4]),
            None => info!("tower: Finalized ep {episode_id} (no prior state recorded)"),
        }
    }
}
