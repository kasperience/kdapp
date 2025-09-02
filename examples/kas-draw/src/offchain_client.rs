use kdapp::engine::EpisodeMessage;

use crate::episode::LotteryEpisode;
use crate::tlv::{MsgType, TlvMsg, TLV_VERSION};

pub fn auto_seq(episode_id: u64, typ: &str) -> u64 {
    let mut store = read_seq_store();
    let next = match (store.get(&episode_id).copied(), typ) {
        (Some(last), _) => last.saturating_add(1),
        (None, "new") => 0,
        _ => 1,
    };
    store.insert(episode_id, next);
    write_seq_store(&store);
    next
}

pub fn send_tlv_cmd(dest: &str, episode_id: u64, seq: u64, msg: EpisodeMessage<LotteryEpisode>, wait_ack: bool) {
    let payload = borsh::to_vec(&msg).unwrap();
    let tlv = TlvMsg {
        version: TLV_VERSION,
        msg_type: MsgType::Cmd as u8,
        episode_id,
        seq,
        state_hash: [0u8; 32],
        payload,
    };
    send_with_ack(dest, tlv, false, wait_ack);
}

pub fn send_tlv_new(dest: &str, episode_id: u64, seq: u64, msg: EpisodeMessage<LotteryEpisode>, wait_ack: bool) {
    let payload = borsh::to_vec(&msg).unwrap();
    let tlv = TlvMsg {
        version: TLV_VERSION,
        msg_type: MsgType::New as u8,
        episode_id,
        seq,
        state_hash: [0u8; 32],
        payload,
    };
    send_with_ack(dest, tlv, false, wait_ack);
}

pub fn send_tlv_close(dest: &str, episode_id: u64, seq: u64, wait_ack: bool) {
    let tlv = TlvMsg {
        version: TLV_VERSION,
        msg_type: MsgType::Close as u8,
        episode_id,
        seq,
        state_hash: [0u8; 32],
        payload: vec![],
    };
    send_with_ack(dest, tlv, true, wait_ack);
}

pub fn send_with_ack(dest: &str, tlv: TlvMsg, expect_close_ack: bool, wait_ack: bool) {
    use std::net::UdpSocket;
    use std::time::Duration;
    let sock = UdpSocket::bind("0.0.0.0:0").expect("bind sender");
    let expected_type = if expect_close_ack { MsgType::AckClose as u8 } else { MsgType::Ack as u8 };

    let attempts = if wait_ack { 3 } else { 1 };
    let mut timeout_ms = 300u64;
    let bytes = tlv.encode();
    for attempt in 0..attempts {
        let _ = sock.send_to(&bytes, dest);
        if !wait_ack {
            break;
        }
        let _ = sock.set_read_timeout(Some(Duration::from_millis(timeout_ms)));
        let mut buf = [0u8; 1024];
        if let Ok((n, _from)) = sock.recv_from(&mut buf) {
            if let Some(ack) = TlvMsg::decode(&buf[..n]) {
                if ack.msg_type == expected_type && ack.episode_id == tlv.episode_id && ack.seq == tlv.seq {
                    println!("ack received for ep {} seq {}", tlv.episode_id, tlv.seq);
                    return;
                }
            }
        }
        if attempt + 1 < attempts {
            timeout_ms = timeout_ms.saturating_mul(2);
            println!("ack timeout, retrying (attempt {} of {})", attempt + 2, attempts);
        } else {
            eprintln!("ack failed for ep {} seq {} (no response)", tlv.episode_id, tlv.seq);
        }
    }
}

fn read_seq_store() -> std::collections::HashMap<u64, u64> {
    use std::io::Read;
    let mut m = std::collections::HashMap::new();
    let path = seq_store_path();
    if let Ok(mut f) = std::fs::File::open(path) {
        let mut s = String::new();
        if f.read_to_string(&mut s).is_ok() {
            for line in s.lines() {
                let parts: Vec<&str> = line.trim().split(',').collect();
                if parts.len() == 2 {
                    if let (Ok(eid), Ok(seq)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) {
                        m.insert(eid, seq);
                    }
                }
            }
        }
    }
    m
}

fn write_seq_store(m: &std::collections::HashMap<u64, u64>) {
    use std::io::Write;
    let mut s = String::new();
    let mut keys: Vec<_> = m.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let v = m.get(&k).copied().unwrap_or(0);
        s.push_str(&format!("{k},{v}\n"));
    }
    let path = seq_store_path();
    if let Ok(mut f) = std::fs::File::create(path) {
        let _ = f.write_all(s.as_bytes());
    }
}

fn seq_store_path() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from("target");
    let _ = std::fs::create_dir_all(&p);
    p.push("kas_draw_offchain_seq.txt");
    p
}

