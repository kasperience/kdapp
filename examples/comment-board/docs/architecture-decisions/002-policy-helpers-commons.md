# ADR-002: Shared Policy Helpers (commons) for Descriptor Verification and Script Templates

- Status: Proposed
- Date: 2025-08-21
- Owners: kdapp devs (fork); upstream proposal TBD

## Context

Episodes increasingly need to validate on-chain bond policies (e.g., P2PK vs timelock vs multisig) using `PayloadMetadata.tx_outputs.script_bytes`. Multiple episode types (Comment Board, future Poker Tournament, Lottery) would duplicate descriptor parsing, policy verification, and script template building.

## Decision (Proposed)

Introduce a shared "policy helpers" module (initially in examples/common/, later upstreamed or moved under kdapp when approved) to centralize:

- Descriptor model: represent intended bond policies in code.
- On-chain decode: parse `script_bytes` into a descriptor-like abstraction.
- Verification: compare declared descriptor vs on-chain bytes.
- Builders (feature-gated/experimental): construct standard, node-accepted script templates (P2PK, timelock, multisig) once finalized.

Initial location options (non-breaking, fork-friendly):
- `examples/common/policy.rs` (simple module shared by examples)
- or a small helper crate under `examples/common/policy-helpers/`

When upstream is ready, consider `kdapp/src/policy.rs`.

## API Sketch

```
// High-level policy intent declared by episodes/CLI
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptPolicy {
    P2pk { pubkey: [u8; 33] },
    Timelock { lock_time: u64, beneficiary: [u8; 33] },
    Multisig { m: u8, pubkeys: Vec<[u8; 33]> },
}

#[derive(thiserror::Error, Debug)]
pub enum PolicyError {
    #[error("unsupported or unknown descriptor")] Unknown,
    #[error("mismatch: declared {declared:?} vs on-chain {observed:?}")]
    Mismatch { declared: ScriptPolicy, observed: ScriptPolicy },
    #[error("malformed script bytes")] Malformed,
}

// Decode raw script bytes (from tx_outputs) into a high-level policy
pub fn decode_script(bytes: &[u8]) -> Result<ScriptPolicy, PolicyError> { /* impl TBD */ }

// Verify declared policy matches on-chain script bytes
pub fn verify_policy(declared: &ScriptPolicy, script_bytes: &[u8]) -> Result<(), PolicyError> { /* impl TBD */ }

// Builders for standard, node-accepted template scripts (feature-gated)
#[cfg(feature = "policy-builders")]
pub mod builders {
    pub fn p2pk(pubkey: &[u8; 33]) -> Vec<u8> { /* impl TBD */ }
    pub fn timelock(unlock_time: u64, beneficiary: &[u8; 33]) -> Vec<u8> { /* impl TBD */ }
    pub fn multisig(m: u8, pubkeys: &[[u8; 33]]) -> Vec<u8> { /* impl TBD */ }
}

// Integration path inside episodes
override fn execute(&mut self, cmd: &Command, auth: Option<PubKey>, meta: &PayloadMetadata) -> Result<Rollback, EpisodeError<Err>> {
    if let Some(outs) = &meta.tx_outputs {
        if let Some(script) = outs.get(0).and_then(|o| o.script_bytes.as_ref()) {
            policy_helpers::verify_policy(&declared, script)?;
        }
    }
    // ... continue
}
```

## Rationale

- Avoid duplication: a single, tested verifier reduces drift across examples.
- Clear boundary: episodes declare intent; verifier checks chain reality.
- Future-proof: builders allow standard templates once public node policy stabilizes.

## Risks & Mitigations

- Node standardness: non-standard scripts will be rejected; keep builders behind feature flags until accepted templates are finalized.
- Upstream alignment: keep helpers in examples/common first; upstream later via PR with migration notes.
- Versioning: gate changes with cargo features; document breaking changes in README.

## Test Plan

- Unit tests: decode_script for known P2PK/timelock/multisig patterns.
- Negative tests: malformed bytes, mismatched policies.
- E2E (engine-level): episodes reject when declared policy != on-chain `script_bytes`.

## Migration Plan

1. Add `examples/common/policy.rs` (or small helper crate) with `decode_script` and `verify_policy`.
2. Wire Comment Board to call `verify_policy` when `tx_outputs.script_bytes` available.
3. Extend to Lottery and Poker Episode once live.
4. Propose upstream move to `kdapp/src/policy.rs` after stabilization.

