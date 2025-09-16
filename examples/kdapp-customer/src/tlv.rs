use blake2::{Blake2b512, Digest};

pub const DEMO_HMAC_KEY: &[u8] = b"kdapp-demo-secret";

pub const TLV_VERSION: u8 = 1;
pub const SCRIPT_POLICY_VERSION: u16 = 1;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum MsgType {
    New = 0,
    Cmd = 1,
    Ack = 2,
    Close = 3,
    AckClose = 4,
    Checkpoint = 5,
    Handshake = 6,
    SubCharge = 7,
    SubChargeAck = 8,
    SubDispute = 9,
    SubDisputeResolve = 10,
}

impl MsgType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(MsgType::New),
            1 => Some(MsgType::Cmd),
            2 => Some(MsgType::Ack),
            3 => Some(MsgType::Close),
            4 => Some(MsgType::AckClose),
            5 => Some(MsgType::Checkpoint),
            6 => Some(MsgType::Handshake),
            7 => Some(MsgType::SubCharge),
            8 => Some(MsgType::SubChargeAck),
            9 => Some(MsgType::SubDispute),
            10 => Some(MsgType::SubDisputeResolve),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct TlvMsg {
    pub version: u8,
    pub msg_type: u8,
    pub script_policy_version: u16,
    pub episode_id: u64,
    pub seq: u64,
    pub state_hash: [u8; 32],
    pub payload: Vec<u8>,
    pub auth: [u8; 32],
}

impl TlvMsg {
    fn bytes_for_sign(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(1 + 1 + 2 + 8 + 8 + 32 + 2 + self.payload.len());
        v.push(self.version);
        v.push(self.msg_type);
        v.extend_from_slice(&self.script_policy_version.to_le_bytes());
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
        if bytes.len() < 1 + 1 + 2 + 8 + 8 + 32 + 2 + 32 {
            return None;
        }
        let version = bytes[0];
        let msg_type = bytes[1];
        if version != TLV_VERSION {
            return None;
        }
        MsgType::from_u8(msg_type)?;
        let script_policy_version = u16::from_le_bytes(bytes[2..4].try_into().ok()?);
        let episode_id = u64::from_le_bytes(bytes[4..12].try_into().ok()?);
        let seq = u64::from_le_bytes(bytes[12..20].try_into().ok()?);
        let mut state_hash = [0u8; 32];
        state_hash.copy_from_slice(&bytes[20..52]);
        let payload_len = u16::from_le_bytes(bytes[52..54].try_into().ok()?);
        if bytes.len() < 54 + payload_len as usize + 32 {
            return None;
        }
        let payload_start = 54;
        let payload_end = payload_start + payload_len as usize;
        let payload = bytes[payload_start..payload_end].to_vec();
        let mut auth = [0u8; 32];
        auth.copy_from_slice(&bytes[payload_end..payload_end + 32]);
        Some(Self { version, msg_type, script_policy_version, episode_id, seq, state_hash, payload, auth })
    }
}

#[allow(dead_code)]
pub fn hash_state(bytes: &[u8]) -> [u8; 32] {
    let mut h = Blake2b512::new();
    h.update(bytes);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out[..32]);
    arr
}
