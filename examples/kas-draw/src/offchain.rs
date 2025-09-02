use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};

use kaspa_consensus_core::Hash;
use kdapp::engine::EngineMsg;
use kdapp::episode::TxOutputInfo;
use log::{info, warn};

use crate::tlv::{MsgType, TlvMsg, TLV_VERSION};

pub struct OffchainRouter {
    last_seq: Arc<Mutex<HashMap<u64, u64>>>,
    sender: std::sync::mpsc::Sender<EngineMsg>,
    ack_enabled: bool,
    close_enabled: bool,
}

impl OffchainRouter {
    pub fn new(sender: std::sync::mpsc::Sender<EngineMsg>, ack_enabled: bool, close_enabled: bool) -> Self {
        Self { last_seq: Arc::new(Mutex::new(HashMap::new())), sender, ack_enabled, close_enabled }
    }

    pub fn run_udp(&self, bind: &str) {
        let sock = UdpSocket::bind(bind).expect("bind offchain router");
        info!("offchain router listening on {bind}");
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let (n, src) = match sock.recv_from(&mut buf) {
                Ok(x) => x,
                Err(e) => {
                    warn!("router recv error: {e}");
                    eprintln!("offchain-router: recv error: {e}");
                    continue;
                }
            };
            let bytes = &buf[..n];
            let Some(msg) = TlvMsg::decode(bytes) else {
                warn!("router: invalid TLV from {src} (len={n})");
                eprintln!("offchain-router: invalid TLV from {src} (len={n})");
                continue;
            };
            if msg.version != TLV_VERSION {
                warn!("router: bad version from {src}");
                eprintln!("offchain-router: bad version from {src}");
                continue;
            }
            let Some(mt) = MsgType::from_u8(msg.msg_type) else {
                warn!("router: bad msg type from {src}");
                eprintln!("offchain-router: bad msg type from {src}");
                continue;
            };

            let mut map = self.last_seq.lock().unwrap();
            let last = map.get(&msg.episode_id).copied();
            let (accepted, is_close) = match mt {
                MsgType::New => {
                    // Strict: only accept New when no prior state and seq == 0
                    if last.is_none() && msg.seq == 0 {
                        map.insert(msg.episode_id, 0);
                        (true, false)
                    } else {
                        warn!("router: reject NEW for ep {} (seq {}), last={:?}", msg.episode_id, msg.seq, last);
                        eprintln!("offchain-router: reject NEW for ep {} (seq {}), last={:?}", msg.episode_id, msg.seq, last);
                        (false, false)
                    }
                }
                MsgType::Cmd => match last {
                    Some(prev) if msg.seq == prev + 1 => {
                        map.insert(msg.episode_id, msg.seq);
                        (true, false)
                    }
                    Some(prev) => {
                        warn!("router: stale/out-of-order CMD ep {} (got {}, want {})", msg.episode_id, msg.seq, prev + 1);
                        eprintln!("offchain-router: stale/out-of-order CMD ep {} (got {}, want {})", msg.episode_id, msg.seq, prev + 1);
                        (false, false)
                    }
                    None => {
                        warn!("router: CMD before NEW for ep {} (got seq {} but no state)", msg.episode_id, msg.seq);
                        eprintln!("offchain-router: CMD before NEW for ep {} (got seq {} but no state)", msg.episode_id, msg.seq);
                        (false, false)
                    }
                },
                MsgType::Ack => {
                    info!("router: ack from {src} ignored");
                    (false, false)
                }
                MsgType::Close => {
                    if !self.close_enabled {
                        warn!("router: close ignored (disabled)");
                        eprintln!("offchain-router: close ignored (disabled)");
                        (false, true)
                    } else {
                        match last {
                            Some(prev) if msg.seq == prev + 1 => {
                                // advance to final state number; removal after ack
                                map.insert(msg.episode_id, msg.seq);
                                (true, true)
                            }
                            Some(prev) => {
                                warn!("router: stale/out-of-order CLOSE ep {} (got {}, want {})", msg.episode_id, msg.seq, prev + 1);
                                eprintln!("offchain-router: stale/out-of-order CLOSE ep {} (got {}, want {})", msg.episode_id, msg.seq, prev + 1);
                                (false, true)
                            }
                            None => {
                                warn!("router: CLOSE before NEW for ep {} (got seq {} but no state)", msg.episode_id, msg.seq);
                                eprintln!("offchain-router: CLOSE before NEW for ep {} (got seq {} but no state)", msg.episode_id, msg.seq);
                                (false, true)
                            }
                        }
                    }
                }
                MsgType::AckClose => {
                    info!("router: ack-close from {src} ignored");
                    (false, false)
                }
                MsgType::Checkpoint => match last {
                    Some(prev) if msg.seq == prev + 1 => {
                        map.insert(msg.episode_id, msg.seq);
                        (true, false)
                    }
                    Some(prev) => {
                        warn!("router: out-of-order CKPT ep {} (got {}, want {})", msg.episode_id, msg.seq, prev + 1);
                        eprintln!("offchain-router: out-of-order CKPT ep {} (got {}, want {})", msg.episode_id, msg.seq, prev + 1);
                        (false, false)
                    }
                    None => {
                        warn!("router: CKPT before NEW for ep {} (seq {})", msg.episode_id, msg.seq);
                        eprintln!("offchain-router: CKPT before NEW for ep {} (seq {})", msg.episode_id, msg.seq);
                        (false, false)
                    }
                },
            };
            drop(map);

            if !accepted {
                continue;
            }

            // Forward to engine as a synthetic block-accepted event
            let accepting_hash: Hash = Hash::default();
            let tx_id: Hash = Hash::default();
            let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            // Wrap as single-item batch. For Checkpoint, we don't forward to engine — it's for watchers only.
            let payload = if matches!(mt, MsgType::Close) {
                // Convert Close TLV into an unsigned CloseEpisode command
                let eid = msg.episode_id as u32;
                let cmd = crate::episode::LotteryCommand::CloseEpisode;
                borsh::to_vec(&kdapp::engine::EpisodeMessage::<crate::episode::LotteryEpisode>::UnsignedCommand {
                    episode_id: eid,
                    cmd,
                })
                .unwrap()
            } else if matches!(mt, MsgType::Checkpoint) {
                Vec::new()
            } else {
                msg.payload
            };
            if !matches!(mt, MsgType::Checkpoint) {
                let event = EngineMsg::BlkAccepted {
                    accepting_hash,
                    accepting_daa: msg.seq,
                    accepting_time: now_secs,
                    associated_txs: vec![(tx_id, payload, None::<Vec<TxOutputInfo>>)],
                };
                if let Err(e) = self.sender.send(event) {
                    warn!("router: failed forwarding to engine: {e}");
                    continue;
                }
            }

            // Ack to sender if enabled and only after successful forward
            if self.ack_enabled {
                let ack_type = if is_close { crate::tlv::MsgType::AckClose as u8 } else { crate::tlv::MsgType::Ack as u8 };
                let ack = crate::tlv::TlvMsg {
                    version: crate::tlv::TLV_VERSION,
                    msg_type: ack_type,
                    episode_id: msg.episode_id,
                    seq: msg.seq,
                    state_hash: msg.state_hash,
                    payload: vec![],
                };
                let _ = sock.send_to(&ack.encode(), src);
            }

            // If this was a Close, clear sequence tracking now that it’s forwarded and acked
            if is_close {
                let mut map = self.last_seq.lock().unwrap();
                map.remove(&msg.episode_id);
            }
        }
    }
}
