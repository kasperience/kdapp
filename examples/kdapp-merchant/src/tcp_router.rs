use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use kaspa_consensus_core::Hash;
use kdapp::engine::EngineMsg;
use kdapp::episode::TxOutputInfo;
use log::{info, warn};

use crate::{
    sim_router::EngineChannel,
    tlv::{MsgType, TlvMsg, TLV_VERSION, DEMO_HMAC_KEY},
};

struct AckState {
    seq: u64,
    ack: Vec<u8>,
}

/// TCP router mirroring the semantics of `UdpRouter`.
/// Accepts TLV messages over a TCP stream and forwards them to the engine.
pub struct TcpRouter {
    last_seq: Arc<Mutex<HashMap<u64, AckState>>>,
    sender: EngineChannel,
}

impl TcpRouter {
    pub fn new(sender: EngineChannel) -> Self {
        Self { last_seq: Arc::new(Mutex::new(HashMap::new())), sender }
    }

    pub fn run(&self, bind: &str) {
        let listener = TcpListener::bind(bind).expect("bind tcp router");
        info!("tcp router listening on {bind}");
        for stream in listener.incoming() {
            match stream {
                Ok(mut s) => {
                    let _ = self.handle_stream(&mut s);
                }
                Err(e) => warn!("router accept error: {e}"),
            }
        }
    }

    fn handle_stream(&self, stream: &mut TcpStream) -> std::io::Result<()> {
        let mut header = [0u8; 52];
        loop {
            if stream.read_exact(&mut header).is_err() {
                break;
            }
            let payload_len = u16::from_le_bytes([header[50], header[51]]) as usize;
            let mut tail = vec![0u8; payload_len + 32];
            if stream.read_exact(&mut tail).is_err() {
                break;
            }
            let mut msg_bytes = header.to_vec();
            msg_bytes.extend_from_slice(&tail);
            let mut msg = match TlvMsg::decode(&msg_bytes) {
                Some(m) => m,
                None => {
                    warn!("router: invalid TLV from tcp peer");
                    continue;
                }
            };
            if msg.version != TLV_VERSION {
                warn!("router: bad version from tcp peer");
                continue;
            }
            let Some(mt) = MsgType::from_u8(msg.msg_type) else {
                warn!("router: bad msg type from tcp peer");
                continue;
            };
            if !msg.verify(DEMO_HMAC_KEY) {
                warn!("router: bad auth from tcp peer");
                continue;
            }

            let mut map = self.last_seq.lock().unwrap();
            let entry = map.get(&msg.episode_id).cloned();
            let mut accepted = false;
            match mt {
                MsgType::New => match entry {
                    None if msg.seq == 0 => accepted = true,
                    Some(info) if msg.seq == info.seq => {
                        let _ = stream.write_all(&info.ack);
                        continue;
                    }
                    _ => warn!("router: reject NEW for ep {} (seq {}), last={:?}", msg.episode_id, msg.seq, entry.map(|i| i.seq)),
                },
                MsgType::Cmd | MsgType::Close | MsgType::Checkpoint => match entry {
                    Some(info) if msg.seq == info.seq + 1 => accepted = true,
                    Some(info) if msg.seq == info.seq => {
                        let _ = stream.write_all(&info.ack);
                        continue;
                    }
                    Some(info) => warn!("router: out-of-order ep {} (got {}, want {})", msg.episode_id, msg.seq, info.seq + 1),
                    None => warn!("router: {} before NEW for ep {} (seq {})", msg.msg_type, msg.episode_id, msg.seq),
                },
                MsgType::Ack | MsgType::AckClose => info!("router: ignoring ack-type from tcp peer"),
            }

            if !accepted {
                continue;
            }
            if !matches!(mt, MsgType::Checkpoint) {
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
            ack.sign(DEMO_HMAC_KEY);
            let ack_bytes = ack.encode();
            let _ = stream.write_all(&ack_bytes);
            map.insert(msg.episode_id, AckState { seq: msg.seq, ack: ack_bytes });
        }
        Ok(())
    }
}

