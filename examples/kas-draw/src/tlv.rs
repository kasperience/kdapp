use blake2::{Blake2b512, Digest};

pub const TLV_VERSION: u8 = 1;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum MsgType {
    New = 0,
    Cmd = 1,
    Ack = 2,
    Close = 3,
}

#[derive(Clone)]
pub struct TlvMsg {
    pub version: u8,      // = TLV_VERSION
    pub msg_type: u8,     // MsgType as u8
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

