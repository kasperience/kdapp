use blake2::{Blake2b512, Digest};
use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::pki::PubKey;
use log::info;
use std::net::UdpSocket;
use std::time::Duration;

pub const DEMO_HMAC_KEY: &[u8] = b"kdapp-demo-secret";
pub const TLV_VERSION: u8 = 1;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum MsgType {
    Handshake = 0,
    Escalate = 1,
    Confirm = 2,
    Ack = 3,
}

impl MsgType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(MsgType::Handshake),
            1 => Some(MsgType::Escalate),
            2 => Some(MsgType::Confirm),
            3 => Some(MsgType::Ack),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct TlvMsg {
    pub version: u8,
    pub msg_type: u8,
    pub episode_id: u64,
    pub seq: u64,
    pub state_hash: [u8; 32],
    pub payload: Vec<u8>,
    pub auth: [u8; 32],
}

impl TlvMsg {
    fn bytes_for_sign(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(1 + 1 + 8 + 8 + 32 + 2 + self.payload.len());
        v.push(self.version);
        v.push(self.msg_type);
        v.extend_from_slice(&self.episode_id.to_le_bytes());
        v.extend_from_slice(&self.seq.to_le_bytes());
        v.extend_from_slice(&self.state_hash);
        let len: u16 = self.payload.len() as u16;
        v.extend_from_slice(&len.to_le_bytes());
        v.extend_from_slice(&self.payload);
        v
    }

    pub fn sign(&mut self, key: &[u8]) {
        let mut h = Blake2b512::new_with_prefix(key);
        h.update(self.bytes_for_sign());
        let out = h.finalize();
        self.auth.copy_from_slice(&out[..32]);
    }

    pub fn verify(&self, key: &[u8]) -> bool {
        let mut h = Blake2b512::new_with_prefix(key);
        h.update(self.bytes_for_sign());
        let out = h.finalize();
        self.auth == out[..32]
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = self.bytes_for_sign();
        v.extend_from_slice(&self.auth);
        v
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 1 + 1 + 8 + 8 + 32 + 2 + 32 {
            return None;
        }
        let version = bytes[0];
        let msg_type = bytes[1];
        if version != TLV_VERSION {
            return None;
        }
        MsgType::from_u8(msg_type)?;
        let episode_id = u64::from_le_bytes(bytes[2..10].try_into().ok()?);
        let seq = u64::from_le_bytes(bytes[10..18].try_into().ok()?);
        let mut state_hash = [0u8; 32];
        state_hash.copy_from_slice(&bytes[18..50]);
        let payload_len = u16::from_le_bytes(bytes[50..52].try_into().ok()?);
        if bytes.len() < 52 + payload_len as usize + 32 {
            return None;
        }
        let payload_start = 52;
        let payload_end = payload_start + payload_len as usize;
        let payload = bytes[payload_start..payload_end].to_vec();
        let mut auth = [0u8; 32];
        auth.copy_from_slice(&bytes[payload_end..payload_end + 32]);
        Some(Self { version, msg_type, episode_id, seq, state_hash, payload, auth })
    }
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum GuardianMsg {
    Handshake { merchant: PubKey, guardian: PubKey },
    Escalate { episode_id: u64, reason: String },
    Confirm { episode_id: u64, seq: u64 },
}

#[derive(Default, Debug)]
pub struct GuardianState {
    pub observed_payments: Vec<u64>,
    pub checkpoints: Vec<(u64, u64)>,
}

impl GuardianState {
    pub fn observe_payment(&mut self, invoice_id: u64) {
        self.observed_payments.push(invoice_id);
    }

    pub fn record_checkpoint(&mut self, episode_id: u64, seq: u64) {
        self.checkpoints.push((episode_id, seq));
    }
}

fn send_with_retry(dest: &str, mut tlv: TlvMsg, key: &[u8]) {
    tlv.sign(key);
    let sock = UdpSocket::bind("0.0.0.0:0").expect("bind sender");
    let bytes = tlv.encode();
    let expected = MsgType::Ack as u8;
    let mut timeout_ms = 300u64;
    for attempt in 0..3 {
        let _ = sock.send_to(&bytes, dest);
        let _ = sock.set_read_timeout(Some(Duration::from_millis(timeout_ms)));
        let mut buf = [0u8; 1024];
        if let Ok((n, _)) = sock.recv_from(&mut buf) {
            if let Some(ack) = TlvMsg::decode(&buf[..n]) {
                if ack.msg_type == expected && ack.episode_id == tlv.episode_id && ack.seq == tlv.seq && ack.verify(key) {
                    return;
                }
            }
        }
        timeout_ms = timeout_ms.saturating_mul(2);
        if attempt < 2 {
            info!("ack timeout, retrying (attempt {} of 3)", attempt + 2);
        }
    }
    info!("ack failed for ep {} seq {}", tlv.episode_id, tlv.seq);
}

fn send_msg(dest: &str, msg: GuardianMsg, key: &[u8]) {
    let (msg_type, episode_id, seq) = match &msg {
        GuardianMsg::Handshake { .. } => (MsgType::Handshake, 0, 0),
        GuardianMsg::Escalate { episode_id, .. } => (MsgType::Escalate, *episode_id, 0),
        GuardianMsg::Confirm { episode_id, seq } => (MsgType::Confirm, *episode_id, *seq),
    };
    let payload = borsh::to_vec(&msg).expect("serialize guardian msg");
    let tlv = TlvMsg {
        version: TLV_VERSION,
        msg_type: msg_type as u8,
        episode_id,
        seq,
        state_hash: [0u8; 32],
        payload,
        auth: [0u8; 32],
    };
    send_with_retry(dest, tlv, key);
}

pub fn handshake(dest: &str, merchant: PubKey, guardian: PubKey, key: &[u8]) {
    send_msg(dest, GuardianMsg::Handshake { merchant, guardian }, key);
}

pub fn send_escalate(dest: &str, episode_id: u64, reason: String, key: &[u8]) {
    send_msg(dest, GuardianMsg::Escalate { episode_id, reason }, key);
}

pub fn send_confirm(dest: &str, episode_id: u64, seq: u64, key: &[u8]) {
    send_msg(dest, GuardianMsg::Confirm { episode_id, seq }, key);
}

pub fn receive(sock: &UdpSocket, state: &mut GuardianState, key: &[u8]) -> Option<GuardianMsg> {
    let mut buf = [0u8; 1024];
    let (n, addr) = sock.recv_from(&mut buf).ok()?;
    let tlv = TlvMsg::decode(&buf[..n])?;
    if !tlv.verify(key) {
        return None;
    }
    let msg: GuardianMsg = borsh::from_slice(&tlv.payload).ok()?;
    match &msg {
        GuardianMsg::Escalate { episode_id, .. } => state.observe_payment(*episode_id),
        GuardianMsg::Confirm { episode_id, seq } => state.record_checkpoint(*episode_id, *seq),
        GuardianMsg::Handshake { .. } => {}
    }
    let mut ack = TlvMsg {
        version: TLV_VERSION,
        msg_type: MsgType::Ack as u8,
        episode_id: tlv.episode_id,
        seq: tlv.seq,
        state_hash: [0u8; 32],
        payload: vec![],
        auth: [0u8; 32],
    };
    ack.sign(key);
    let _ = sock.send_to(&ack.encode(), addr);
    Some(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdapp::pki::generate_keypair;

    #[test]
    fn handshake_roundtrip() {
        let (_sk_g, pk_g) = generate_keypair();
        let (_sk_m, pk_m) = generate_keypair();
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut state = GuardianState::default();
            let msg = receive(&server, &mut state, DEMO_HMAC_KEY).unwrap();
            assert!(matches!(msg, GuardianMsg::Handshake { .. }));
        });
        handshake(&addr.to_string(), pk_m, pk_g, DEMO_HMAC_KEY);
        handle.join().unwrap();
    }

    #[test]
    fn escalation_roundtrip() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut state = GuardianState::default();
            let msg1 = receive(&server, &mut state, DEMO_HMAC_KEY).unwrap();
            assert!(matches!(msg1, GuardianMsg::Escalate { .. }));
            let msg2 = receive(&server, &mut state, DEMO_HMAC_KEY).unwrap();
            assert!(matches!(msg2, GuardianMsg::Confirm { .. }));
            state
        });
        send_escalate(&addr.to_string(), 42, "late payment".to_string(), DEMO_HMAC_KEY);
        send_confirm(&addr.to_string(), 42, 7, DEMO_HMAC_KEY);
        let state = handle.join().unwrap();
        assert_eq!(state.observed_payments, vec![42]);
        assert_eq!(state.checkpoints, vec![(42, 7)]);
    }
}
