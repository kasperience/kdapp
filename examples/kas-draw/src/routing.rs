use kdapp::generator::{PatternType, PrefixType};

// Centralized routing for kas-draw.
// Prefix identifies this episode family. Pattern is derived deterministically
// from the prefix to avoid copy-paste drift across modules.
pub const PREFIX: PrefixType = u32::from_le_bytes(*b"KDRW");
pub const CHECKPOINT_PREFIX: PrefixType = u32::from_le_bytes(*b"KDCK");

pub fn pattern() -> PatternType {
    // Simple derivation: alternate bit values using low 10 bits of the prefix
    // so that PATTERN is stable given the chosen prefix.
    let bits = (0..10)
        .map(|i| {
            let v = ((PREFIX >> (i % 16)) & 1) as u8; // wrap every 16 bits
            (i as u8, if i % 2 == 0 { v ^ 1 } else { v })
        })
        .collect::<Vec<(u8, u8)>>();
    let mut out: PatternType = [(0, 0); 10];
    for (i, e) in bits.into_iter().enumerate().take(10) {
        out[i] = e;
    }
    out
}

