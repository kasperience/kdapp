use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use kaspa_consensus_core::Hash;
use kdapp::engine::EngineMsg;
use kdapp::episode::TxOutputInfo;
use log::{info, warn};

use crate::{
    sim_router::EngineChannel,
    tlv::{MsgType, TlvMsg, TLV_VERSION},
};

/// Minimal UDP TLV router for off-chain delivery.
/// Accepts TLV messages carrying serialized `EpisodeMessage<ReceiptEpisode>` payloads
/// and forwards them to the engine as synthetic block-accepted events.
#[derive(Clone)]
struct AckState {
    seq: u64,
    ack: Vec<u8>,
}

pub struct UdpRouter {
    last_seq: Arc<Mutex<HashMap<u64, AckState>>>,
    keys: Arc<Mutex<HashMap<SocketAddr, Vec<u8>>>>,
    sender: EngineChannel,
}

impl UdpRouter {
    pub fn new(sender: EngineChannel) -> Self {
        Self { last_seq: Arc::new(Mutex::new(HashMap::new())), keys: Arc::new(Mutex::new(HashMap::new())), sender }
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
            if matches!(mt, MsgType::Handshake) {
                let key = msg.payload.clone();
                {
                    let mut kmap = self.keys.lock().unwrap();
                    kmap.insert(src, key.clone());
                }
                let mut ack = TlvMsg {
                    version: TLV_VERSION,
                    msg_type: MsgType::Ack as u8,
                    episode_id: msg.episode_id,
                    seq: msg.seq,
                    state_hash: msg.state_hash,
                    payload: vec![],
                    auth: [0u8; 32],
                };
                ack.sign(&key);
                let _ = sock.send_to(&ack.encode(), src);
                continue;
            }
            let Some(key) = ({
                let kmap = self.keys.lock().unwrap();
                kmap.get(&src).cloned()
            }) else {
                warn!("router: message from {src} without handshake");
                continue;
            };
            if !msg.verify(&key) {
                warn!("router: bad auth from {src}");
                continue;
            }

            // Simple in-order sequencing per episode
            let mut map = self.last_seq.lock().unwrap();
            let entry = map.get(&msg.episode_id).cloned();
            let mut accepted = false;
            match mt {
                MsgType::New => match entry {
                    None if msg.seq == 0 => {
                        accepted = true;
                    }
                    Some(info) if msg.seq == info.seq => {
                        let _ = sock.send_to(&info.ack, src);
                        continue;
                    }
                    _ => {
                        warn!("router: reject NEW for ep {} (seq {}), last={:?}", msg.episode_id, msg.seq, entry.map(|i| i.seq));
                    }
                },
                MsgType::Cmd | MsgType::Close | MsgType::Checkpoint => match entry {
                    Some(info) if msg.seq == info.seq + 1 => {
                        accepted = true;
                    }
                    Some(info) if msg.seq == info.seq => {
                        let _ = sock.send_to(&info.ack, src);
                        continue;
                    }
                    Some(info) => {
                        warn!("router: out-of-order ep {} (got {}, want {})", msg.episode_id, msg.seq, info.seq + 1);
                    }
                    None => {
                        warn!("router: {} before NEW for ep {} (seq {})", msg.msg_type, msg.episode_id, msg.seq);
                    }
                },
                MsgType::Ack | MsgType::AckClose => {
                    info!("router: ignoring ack-type from {src}");
                }
                MsgType::Handshake => {
                    // Should have been handled earlier; ignore in data phase
                    continue;
                }
            }

            if !accepted {
                continue;
            }

            if !matches!(mt, MsgType::Checkpoint) {
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

            // Ack to sender
            let ack_type = if matches!(mt, MsgType::Close) { MsgType::AckClose as u8 } else { MsgType::Ack as u8 };
            let mut ack = TlvMsg {
                version: TLV_VERSION,
                msg_type: ack_type,
                episode_id: msg.episode_id,
                seq: msg.seq,
                state_hash: msg.state_hash,
                payload: vec![],
                auth: [0u8; 32],
            };
            ack.sign(&key);
            let ack_bytes = ack.encode();
            let _ = sock.send_to(&ack_bytes, src);

            map.insert(msg.episode_id, AckState { seq: msg.seq, ack: ack_bytes });
        }
    }
}

