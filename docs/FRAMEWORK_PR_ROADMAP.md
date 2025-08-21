# kdapp Framework PR Roadmap — TX Context for Episodes

This document outlines the motivation and staged plan for extending kdapp’s
engine/proxy API to include minimal carrier‑transaction context for episode command
verification (economic assertions like “bond_amount == on‑chain value”). The goal is to
keep kdapp/src changes small, optional, and high value.

## Motivation
- Close an integrity gap: today episodes can only trust the command payload for amounts.
  A malicious client could declare a 100 KAS bond but attach a smaller value in the carrier TX.
- Enable verifiable economic contracts: episodes can assert that the on‑chain value and (later)
  script template matches their rules, without additional RPC round‑trips.
- Keep it optional: legacy episodes ignore `tx_outputs`; only apps that need it read it.

## Scope (Minimal Surface)
- Add `TxOutputInfo { value: u64, script_version: u16 }`.
- Extend `PayloadMetadata` with `tx_outputs: Option<Vec<TxOutputInfo>>`.
- Proxy passes outputs for matched transactions; Engine preserves this metadata through
  execution and reorg.

## Staged Plan
1) Value verification (this change)
   - Episodes that need it check `metadata.tx_outputs` to ensure the declared amount is present
     in some output of the carrier TX.

2) Script verification (next)
   - Introduce stable encoding/checks for script policy (e.g., timelock, multisig) using kaspa‑txscript.
   - Maintain a default P2PK bond path (standard‑valid) for broad network compatibility.
   - Enrich `TxOutputInfo` with script bytes or a compact descriptor only when the dependency
     boundary is well‑defined.

3) Exact output matching
   - Commands carry an outpoint hint (index) and episodes match exact output (value + script).
   - Store `utxo_reference` in episode state for later disputes/refunds.

4) Tests and docs
   - Unit tests for proxy→engine wiring and episode checks.
   - Docs update in example apps to demonstrate the pattern.

## Compatibility
- Backwards compatible: existing episodes/tests compile by passing `tx_outputs: None`.
- No behavior change unless an episode opts into reading `tx_outputs`.

## Alternatives Considered
- Per‑episode RPC calls in execute: increases coupling and latency; not ideal for reorg semantics.
- Heavier TX context: start minimal to avoid over‑exposing node internals.

## Request
We propose upstreaming this minimal, optional `tx_outputs` surface to unlock verifiable
economic contracts while maintaining kdapp’s simplicity.
