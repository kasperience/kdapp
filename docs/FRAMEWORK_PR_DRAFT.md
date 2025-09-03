# PR: Optional Script Bytes in Tx Context for Episode Verification

This PR proposes a minimal, backwards-compatible extension to kdapp’s transaction context to
unlock script-policy verification for economic episodes (e.g., timelock/multisig bonds).

## Summary
- Add `script_bytes: Option<Vec<u8>>` to `TxOutputInfo` carried within `PayloadMetadata`.
- Proxy populates `value`, `script_version`, and can optionally include `script_bytes` per output.
- Engine passes `PayloadMetadata` unchanged through execution and reorg (no behavioral change).
- Episodes that opt in can verify the on-chain script (or a compact descriptor embedded in it)
  against the command’s declared intent and reject mismatches.

## Motivation
- Today, economic claims (e.g., `bond_amount`) can be verified against the carrier TX outputs via
  `tx_outputs`. However, script policies (e.g., timelock/multisig rules) cannot be verified
  without the script bytes.
- Providing optional `script_bytes` enables strong, consensus-grade verification while keeping the
  default lightweight path unchanged for episodes that don’t need it.

## Changes
- `kdapp/src/episode.rs`
  - `TxOutputInfo { value: u64, script_version: u16, script_bytes: Option<Vec<u8>> }`.
- `kdapp/src/proxy.rs`
  - Populates `script_version` and leaves `script_bytes: None` by default (safe fallback).
- `kdapp/src/engine.rs`
  - No functional changes; continues to forward `PayloadMetadata`.

## Compatibility
- Backwards-compatible: existing episodes/tests compile without changes.
- Optionality: `script_bytes` can remain `None` with no impact on behavior.

## Example: Comment-Board Episode (consumer)
- Command carries a compact `BondScriptKind` descriptor (P2PK | TimeLock | Multisig | Combined).
- Episode verifies on-chain `bond_amount` via `tx_outputs` and, if `script_bytes` are present,
  decodes an on-chain descriptor and compares it to the command’s intent. Mismatch → reject.
- Default P2PK bond remains standard-valid; experimental script-bonds can be tested behind a flag.

## Alternatives Considered
- Episode-initiated RPC calls during execute: increases latency and couples episodes to node APIs.
- Heavier context surface: start minimal (per-output bytes) and evolve as needed.

## Testing Plan
- Unit: ensure proxy→engine wiring remains stable when `script_bytes` is `None`.
- Integration (when node APIs expose script bytes): verify episode correctly decodes and matches
  descriptors for positive/negative cases.
- Regression: confirm episodes ignoring `script_bytes` behave identically.

## Rollout
- Phase 1: land optional `script_bytes`; no behavior change by default.
- Phase 2: enable proxy population of `script_bytes` where RPC types allow.

## Local Feature Flag (for validation)
- Crate: `kdapp` feature `tx-script-bytes` populates `TxOutputInfo.script_bytes` from
  `out.script_public_key.script()` when available.
- Usage examples:
  - `cargo check -p kdapp --features tx-script-bytes`
  - `cargo build -p kdapp --features tx-script-bytes`
- Episodes can then decode on-chain descriptors and validate against command intent.
- Phase 3: episodes adopt verification (descriptor decode/compare or direct script template checks).

## Notes
- This PR does not prescribe specific script templates; it enables episodes to independently
  verify policy once templates are stable (kaspa-txscript).
- The compact descriptor format is documented in `examples/comment-board/docs/script-descriptor.md`.
