use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::pki::PubKey;
use log::info;

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

pub fn handshake(addr: &str) {
    info!("guardian handshake with {addr}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdapp::pki::generate_keypair;

    #[test]
    fn msg_roundtrip() {
        let (_sk_g, pk_g) = generate_keypair();
        let (_sk_m, pk_m) = generate_keypair();
        let msg = GuardianMsg::Handshake { merchant: pk_m, guardian: pk_g };
        let enc = borsh::to_vec(&msg).unwrap();
        let dec: GuardianMsg = borsh::from_slice(&enc).unwrap();
        assert!(matches!(dec, GuardianMsg::Handshake { .. }));
    }
}
