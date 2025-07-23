use kdapp::generator::{PatternType, PrefixType};

// TODO: derive pattern from prefix (using prefix as a random seed for composing the pattern)
pub const PATTERN: PatternType = [(7, 0), (32, 1), (45, 0), (99, 1), (113, 0), (126, 1), (189, 0), (200, 1), (211, 0), (250, 1)];
pub const PREFIX: PrefixType = 858598618;
pub const FEE: u64 = 5000;