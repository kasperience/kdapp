use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use kaspa_consensus_core::Hash;
use kdapp::engine::EngineMsg;
use kdapp::episode::TxOutputInfo;
use log::{info, warn};

use crate::tlv::{MsgType, TlvMsg, TLV_VERSION};

/// Minimal UDP TLV router for off-chain delivery.
/// Accepts TLV messages carrying serialized `EpisodeMessage<ReceiptEpisode>` payloads
/// and forwards them to the engine as synthetic block-accepted events.
pub struct UdpRouter {
    last_seq: Arc<Mutex<HashMap<u64, u64>>>,
    sender: std::sync::mpsc::Sender<EngineMsg>,
}

impl UdpRouter {
    pub fn new(sender: std::sync::mpsc::Sender<EngineMsg>) -> Self {
        Self { last_seq: Arc::new(Mutex::new(HashMap::new())), sender }
    }

    pub fn run(&self, bind: &str) {
        let sock = UdpSocket::bind(bind).expect("bind udp router");
        info!("udp router listening on {bind}");
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let (n, src) = match sock.recv_from(&mut buf) {
                Ok(x) => x,
                Err(e) => {
                    warn!("router recv error: {e}");
                    continue;
                }
            };
            let bytes = &buf[..n];
            let Some(msg) = TlvMsg::decode(bytes) else {
                warn!("router: invalid TLV from {src} (len={n})");
                continue;
            };
            if msg.version != TLV_VERSION {
                warn!("router: bad version from {src}");
                continue;
            }
            let Some(mt) = MsgType::from_u8(msg.msg_type) else {
                warn!("router: bad msg type from {src}");
                continue;
            };

            // Simple in-order sequencing per episode
            let mut map = self.last_seq.lock().unwrap();
            let last = map.get(&msg.episode_id).copied();
            let accepted = match mt {
                MsgType::New => {
                    if last.is_none() && msg.seq == 0 {
                        map.insert(msg.episode_id, 0);
                        true
                    } else {
                        warn!("router: reject NEW for ep {} (seq {}), last={:?}", msg.episode_id, msg.seq, last);
                        false
                    }
                }
                MsgType::Cmd | MsgType::Close | MsgType::Checkpoint => match last {
                    Some(prev) if msg.seq == prev + 1 => {
                        map.insert(msg.episode_id, msg.seq);
                        true
                    }
                    Some(prev) => {
                        warn!("router: out-of-order ep {} (got {}, want {})", msg.episode_id, msg.seq, prev + 1);
                        false
                    }
                    None => {
                        warn!("router: {} before NEW for ep {} (seq {})", msg.msg_type, msg.episode_id, msg.seq);
                        false
                    }
                },
                MsgType::Ack | MsgType::AckClose => {
                    info!("router: ignoring ack-type from {src}");
                    false
                }
            };
            drop(map);
            if !accepted {
                continue;
            }

            if let MsgType::Checkpoint = mt {
                // Checkpoints are watcher-side; do not forward to engine
                continue;
            }

            // Forward as a synthetic block-accepted event with single payload
            let accepting_hash: Hash = Hash::default();
            let tx_id: Hash = Hash::default();
            let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let event = EngineMsg::BlkAccepted {
                accepting_hash,
                accepting_daa: msg.seq,
                accepting_time: now_secs,
                associated_txs: vec![(tx_id, msg.payload, None::<Vec<TxOutputInfo>>)],
            };
            if let Err(e) = self.sender.send(event) {
                warn!("router: failed forwarding to engine: {e}");
            }
        }
    }
}

