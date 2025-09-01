use blake2::{Blake2b512, Digest};

pub const TLV_VERSION: u8 = 1;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum MsgType {
    New = 0,
    Cmd = 1,
    Ack = 2,
    Close = 3,
    AckClose = 4,
    Checkpoint = 5,
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
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct TlvMsg {
    pub version: u8,  // = TLV_VERSION
    pub msg_type: u8, // MsgType as u8
    pub episode_id: u64,
    pub seq: u64,
    pub state_hash: [u8; 32],
    pub payload: Vec<u8>, // serialized EpisodeMessage
}

impl TlvMsg {
    pub fn encode(&self) -> Vec<u8> {
        // version(1) | type(1) | episode_id(8) | seq(8) | state_hash(32) | payload_len(2) | payload
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

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 1 + 1 + 8 + 8 + 32 + 2 {
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
        if bytes.len() < 52 + payload_len as usize {
            return None;
        }
        let payload = bytes[52..(52 + payload_len as usize)].to_vec();
        Some(Self { version, msg_type, episode_id, seq, state_hash, payload })
    }
}

/// state_hash = BLAKE2b(root(serialized Episode state)) truncated to 32 bytes
pub fn hash_state(bytes: &[u8]) -> [u8; 32] {
    let mut h = Blake2b512::new();
    h.update(bytes);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out[..32]);
    arr
}
