use blake2::{Blake2b512, Digest};
use serde::{Deserialize, Serialize};
use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::pki::{sign_message, to_message, verify_signature, PubKey, Sig};
use secp256k1::{PublicKey, SecretKey, Secp256k1};

/// Demo shared secret used for HMAC signing of TLV messages.
/// In real deployments this should be negotiated out of band.
pub const DEMO_HMAC_KEY: &[u8] = b"kdapp-demo-secret";

pub const TLV_VERSION: u8 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MsgType {
    New = 0,
    Cmd = 1,
    Ack = 2,
    Close = 3,
    AckClose = 4,
    Checkpoint = 5,
    Handshake = 6,
    Refund = 7,
    SubCharge = 8,
    SubChargeAck = 9,
    SubDispute = 10,
    SubDisputeResolve = 11,
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
            7 => Some(MsgType::Refund),
            8 => Some(MsgType::SubCharge),
            9 => Some(MsgType::SubChargeAck),
            10 => Some(MsgType::SubDispute),
            11 => Some(MsgType::SubDisputeResolve),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SubCharge {
    pub sub_id: u64,
    pub period_start_ts: u64,
    pub period_end_ts: u64,
    pub expected_amount: u64,
    pub invoice_id: u64,
    pub merchant_pubkey: Vec<u8>,
    pub hmac: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SubChargeAck {
    pub sub_id: u64,
    pub ok: bool,
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SubDispute {
    pub sub_id: u64,
    pub invoice_id: u64,
    pub reason: String,
    pub evidence_hash: Vec<u8>,
    pub proposed_refund_tx: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SubDisputeResolve {
    pub sub_id: u64,
    pub invoice_id: u64,
    pub guardian_sig: Vec<u8>,
    pub refund_tx: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct TlvMsg {
    pub version: u8,  // = TLV_VERSION
    pub msg_type: u8, // MsgType as u8
    pub episode_id: u64,
    pub seq: u64,
    pub state_hash: [u8; 32],
    pub payload: Vec<u8>, // serialized EpisodeMessage
    pub auth: [u8; 32],   // HMAC over header+payload
}

impl TlvMsg {
    fn bytes_for_sign(&self) -> Vec<u8> {
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

    /// Compute the HMAC for this message and store it in `auth`.
    pub fn sign(&mut self, key: &[u8]) {
        // Use a keyed prefix for MAC-like behavior
        let mut h = Blake2b512::new_with_prefix(key);
        h.update(self.bytes_for_sign());
        let out = h.finalize();
        self.auth.copy_from_slice(&out[..32]);
    }

    /// Verify the message HMAC with the provided key.
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

/// state_hash = BLAKE2b(root(serialized Episode state)) truncated to 32 bytes
pub fn hash_state(bytes: &[u8]) -> [u8; 32] {
    let mut h = Blake2b512::new();
    h.update(bytes);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out[..32]);
    arr
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Attestation {
    pub root_hash: [u8; 32],
    pub epoch: u64,
    pub fee_bucket: u64,
    pub congestion_ratio: f64,

    pub attester_pubkey: PubKey,


    pub signature: Vec<u8>,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct AttestationSigData {
    root_hash: [u8; 32],
    epoch: u64,
    fee_bucket: u64,
    congestion_ratio: f64,

    attester_pubkey: PubKey,

}

pub fn sign_attestation(sk: &SecretKey, att: &mut Attestation) {
    let secp = Secp256k1::signing_only();
    let pk = PublicKey::from_secret_key(&secp, sk);

    att.attester_pubkey = PubKey(pk);


    let data = AttestationSigData {
        root_hash: att.root_hash,
        epoch: att.epoch,
        fee_bucket: att.fee_bucket,
        congestion_ratio: att.congestion_ratio,
        attester_pubkey: att.attester_pubkey,
    };
    let msg = to_message(&data);
    let sig = sign_message(sk, &msg);
    att.signature = sig.0.serialize_der().to_vec();
}

pub fn verify_attestation(att: &Attestation) -> bool {

    let pk = match PublicKey::from_slice(&att.attester_pubkey) {
        Ok(k) => k,
        Err(_) => return false,
    };

    let sig = match secp256k1::ecdsa::Signature::from_der(&att.signature) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let data = AttestationSigData {
        root_hash: att.root_hash,
        epoch: att.epoch,
        fee_bucket: att.fee_bucket,
        congestion_ratio: att.congestion_ratio,
        attester_pubkey: att.attester_pubkey,
    };
    let msg = to_message(&data);

    let pk = att.attester_pubkey;

    let pk = PubKey(pk);

    let sig = Sig(sig);
    verify_signature(&pk, &msg, &sig)
}
