use std::net::UdpSocket;
use std::time::Duration;

use kdapp::engine::EpisodeMessage;

use crate::episode::ReceiptEpisode;
use crate::tlv::{MsgType, TlvMsg, TLV_VERSION, DEMO_HMAC_KEY};

/// Send a TLV message over UDP and retry if no acknowledgement is received.
pub fn send_with_retry(dest: &str, mut tlv: TlvMsg, expect_close_ack: bool) {
    tlv.sign(DEMO_HMAC_KEY);
    let sock = UdpSocket::bind("0.0.0.0:0").expect("bind sender");
    let expected = if expect_close_ack { MsgType::AckClose as u8 } else { MsgType::Ack as u8 };
    let bytes = tlv.encode();
    let mut timeout_ms = 300u64;
    for attempt in 0..3 {
        let _ = sock.send_to(&bytes, dest);
        let _ = sock.set_read_timeout(Some(Duration::from_millis(timeout_ms)));
        let mut buf = [0u8; 1024];
        if let Ok((n, _)) = sock.recv_from(&mut buf) {
            if let Some(ack) = TlvMsg::decode(&buf[..n]) {
                if ack.msg_type == expected && ack.episode_id == tlv.episode_id && ack.seq == tlv.seq
                    && ack.verify(DEMO_HMAC_KEY)
                {
                    println!("ack received for ep {} seq {}", tlv.episode_id, tlv.seq);
                    return;
                }
            }
        }
        timeout_ms = timeout_ms.saturating_mul(2);
        if attempt < 2 {
            println!("ack timeout, retrying (attempt {} of 3)", attempt + 2);
        }
    }
    eprintln!("ack failed for ep {} seq {}", tlv.episode_id, tlv.seq);
}

#[allow(dead_code)]
pub fn send_cmd(dest: &str, episode_id: u64, seq: u64, msg: EpisodeMessage<ReceiptEpisode>) {
    let payload = borsh::to_vec(&msg).expect("serialize cmd");
    let tlv = TlvMsg {
        version: TLV_VERSION,
        msg_type: MsgType::Cmd as u8,
        episode_id,
        seq,
        state_hash: [0u8; 32],
        payload,
        auth: [0u8; 32],
    };
    send_with_retry(dest, tlv, false);
}

#[allow(dead_code)]
pub fn send_new(dest: &str, episode_id: u64, seq: u64, msg: EpisodeMessage<ReceiptEpisode>) {
    let payload = borsh::to_vec(&msg).expect("serialize new");
    let tlv = TlvMsg {
        version: TLV_VERSION,
        msg_type: MsgType::New as u8,
        episode_id,
        seq,
        state_hash: [0u8; 32],
        payload,
        auth: [0u8; 32],
    };
    send_with_retry(dest, tlv, false);
}

#[allow(dead_code)]
pub fn send_close(dest: &str, episode_id: u64, seq: u64) {
    let tlv = TlvMsg {
        version: TLV_VERSION,
        msg_type: MsgType::Close as u8,
        episode_id,
        seq,
        state_hash: [0u8; 32],
        payload: vec![],
        auth: [0u8; 32],
    };
    send_with_retry(dest, tlv, true);
}

