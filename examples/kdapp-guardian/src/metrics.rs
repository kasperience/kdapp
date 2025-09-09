use std::sync::atomic::{AtomicU64, Ordering};

pub static VALID: AtomicU64 = AtomicU64::new(0);
pub static INVALID: AtomicU64 = AtomicU64::new(0);

pub fn inc_valid() {
    VALID.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_invalid() {
    INVALID.fetch_add(1, Ordering::Relaxed);
}

pub fn snapshot() -> (u64, u64) {
    (VALID.load(Ordering::Relaxed), INVALID.load(Ordering::Relaxed))
}

#[cfg(test)]
pub fn reset() {
    VALID.store(0, Ordering::Relaxed);
    INVALID.store(0, Ordering::Relaxed);
}
