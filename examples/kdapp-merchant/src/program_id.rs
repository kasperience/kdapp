use kdapp::{generator::{PatternType, PrefixType}, pki::PubKey};
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

/// Derive a unique `PrefixType` and 10-bit `PatternType` from the merchant
/// public key. This keeps routing identifiers deterministic per merchant
/// instance while remaining reproducible across restarts.
pub fn derive_routing_ids(merchant: &PubKey) -> (PrefixType, PatternType) {
    let mut hasher = Sha256::new();
    hasher.update(b"onlyKAS:routing");
    hasher.update(merchant.0.serialize());
    let digest = hasher.finalize();

    // First 4 bytes become the prefix
    let prefix = PrefixType::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);

    // Next 20 bytes derive 10 (pos, bit) pairs
    let mut pattern = [(0u8, 0u8); 10];
    for i in 0..10 {
        let pos = digest[4 + i];
        let bit = digest[14 + i] & 1;
        pattern[i] = (pos, bit);
    }

    (prefix, pattern)
}

