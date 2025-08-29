#[derive(Clone, Debug)]
pub struct Outpoint {
    pub txid: [u8; 32],
    pub vout: u32,
}

#[derive(Clone, Debug)]
pub struct StateBundle {
    pub funding_outpoint: Outpoint,   // channel anchor
    pub episode_id: u64,              // kdapp episode id
    pub state_num: u64,               // monotonically increasing
    pub state_hash: [u8; 32],         // hash of episode state/commit
    pub revocation_secret_prev: [u8; 32], // penalty for stale close
    pub penalty_hint: Vec<u8>,        // compact template or script hint
    pub csv_secs: u32,                // tower reaction window
}

#[derive(Clone, Debug)]
pub struct UtxoWatch {
    pub outpoint: Outpoint,           // locked entry or winner-claim UTXO
    pub expires_at_unix: u64,         // alert/act after this
    pub note: &'static str,           // "locked_entry" | "winner_claim" | etc.
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
    async fn submit_state(&self, _bundle: StateBundle) -> anyhow::Result<()> { Ok(()) }
    async fn watch_utxo(&self, _watch: UtxoWatch) -> anyhow::Result<()> { Ok(()) }
}

