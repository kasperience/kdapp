use std::time::{SystemTime, UNIX_EPOCH};

use kaspa_consensus_core::Hash;
use kdapp::engine::{EngineMsg, EpisodeMessage};
use kdapp::episode::TxOutputInfo;

/// A minimal in-process router that forwards EpisodeMessage payloads
/// to the engine as synthetic block-accepted events.
pub struct SimRouter {
    sender: std::sync::mpsc::Sender<EngineMsg>,
}

impl SimRouter {
    pub fn new(sender: std::sync::mpsc::Sender<EngineMsg>) -> Self { Self { sender } }

    pub fn forward<G: kdapp::episode::Episode>(&self, msg: EpisodeMessage<G>) {
        let payload = borsh::to_vec(&msg).expect("serialize episode msg");
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let accepting_hash = Hash::default();
        let tx_id = Hash::default();
        let event = EngineMsg::BlkAccepted {
            accepting_hash,
            accepting_daa: 0,
            accepting_time: now,
            associated_txs: vec![(tx_id, payload, None::<Vec<TxOutputInfo>>)],
        };
        let _ = self.sender.send(event);
    }
}
