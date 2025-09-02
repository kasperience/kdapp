use kdapp::pki::PubKey;
use sha2::{Digest, Sha256};

/// Derive a deterministic 32-byte label for this merchant program.
/// Placeholder for the more advanced Q = P + H(tag, P||id)*G formulation.
pub fn derive_program_label(merchant: &PubKey, label: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"onlyKAS:program");
    hasher.update(merchant.0.serialize());
    hasher.update(label.as_bytes());
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

