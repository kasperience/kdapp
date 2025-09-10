use std::time::{SystemTime, UNIX_EPOCH};

use kaspa_consensus_core::Hash;
use kdapp::engine::{EngineMsg, EpisodeMessage};
use kdapp::episode::TxOutputInfo;

/// Destination for forwarded engine messages. When `Proxy` is selected the
/// sender should correspond to the channel used by `proxy::run_listener`.
#[derive(Clone)]
pub enum EngineChannel {
    Local(std::sync::mpsc::Sender<EngineMsg>),
    Proxy(std::sync::mpsc::Sender<EngineMsg>),
}

impl EngineChannel {
    pub fn send(&self, msg: EngineMsg) -> Result<(), std::sync::mpsc::SendError<EngineMsg>> {
        match self {
            EngineChannel::Local(tx) | EngineChannel::Proxy(tx) => tx.send(msg),
        }
    }
}

/// A minimal in-process router that forwards EpisodeMessage payloads
/// to the engine as synthetic block-accepted events.
#[derive(Clone)]
pub struct SimRouter {
    sender: EngineChannel,
}

impl SimRouter {
    pub fn new(sender: EngineChannel) -> Self {
        Self { sender }
    }

    pub fn forward<G: kdapp::episode::Episode>(
        &self,
        msg: EpisodeMessage<G>,
    ) -> Result<(), std::sync::mpsc::SendError<EngineMsg>> {
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
        self.sender.send(event)
    }
}
