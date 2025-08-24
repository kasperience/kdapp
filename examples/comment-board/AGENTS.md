# Comment Board — Agents Guide

This document orients contributors and tooling agents working only on `examples/comment-board`.
It summarizes the local architecture, do/don’ts, run instructions, extensibility points, and a
focused roadmap for on‑chain signing and script‑enforced bonds.

## Overview
- Purpose: Decentralized comment room with economic episode contracts on Kaspa L1.
- Status: Phase 2.0 scaffolding present (script builders, UTXO lock manager), with ongoing
  integration toward full script‑enforced bonds and verifiable on‑chain value checks.
- Pattern/Prefix routing: defined in `src/utils/mod.rs` as `PATTERN` and `PREFIX` and wired
  via `proxy::run_listener` and `TransactionGenerator`.

## How It Fits the kdapp Architecture
- `Episode` implementation: `ContractCommentBoard` in `src/episode/board_with_contract.rs`.
  - Validates and updates episode state per command; stores minimal rollback info.
  - Uses `authorization: Option<PubKey>` for per‑command identity (Kaspa key = identity).
- Event handling: `src/episode/handler.rs` pushes `ContractState` to the TUI.
- Routing:
  - `src/utils/mod.rs`: provides `PATTERN`, `PREFIX`, `FEE`.
  - `src/participant/mod.rs`:
    - Starts `Engine::<ContractCommentBoard, CommentHandler>`.
    - Runs `proxy::run_listener(kaspad, [(PREFIX,(PATTERN, tx))], ...)`.
    - Builds episode TXs with `generator::TransactionGenerator::new(keypair, PATTERN, PREFIX)`.
- Wallet/locking:
  - `src/wallet/utxo_manager*.rs`: coin control, micro‑UTXO prep, and bond lock/unlock flows.
  - `src/wallet/kaspa_scripts.rs`: Phase 2.0 script builders (timelock, moderator multisig, combined).

## Run
- Create new room (organizer):
  - `cargo run -p comment-board -- --kaspa-private-key <hex>`
- Join existing room (participant):
  - `cargo run -p comment-board -- --kaspa-private-key <hex> --room-episode-id <id>`
- Options:
  - `--wrpc-url wss://host:port` (defaults to PNN when omitted)
  - `--mainnet` (default is testnet‑10)
  - `--bonds` to enable economic bonds per comment

## Commands and Flow (high level)
- Room/identity:
  - `NewEpisode` → create/register room (episode ID).
  - `JoinRoom { bond_amount }` → join with optional bond commitment.
  - `RequestChallenge` → engine returns nonce; `SubmitResponse { signature, nonce }` → auth.
- Commenting:
  - `SubmitComment { text, bond_amount }` → posts comment; app may require bond.
- Economic life‑cycle (current):
  - App‑level accounting tracks bonds; UTXO manager prepares/locks funds client‑side.
  - Phase 2.0 introduces script‑enforced bonds (timelock OR moderator multisig unlock).

## Do / Don’t (critical)
- Do: Use `TransactionGenerator` only for episode messages (NewEpisode, commands).
- Don’t: Use `TransactionGenerator` for wallet funding, UTXO splitting, or generic payments.
  - Wallet ops go through `UtxoLockManager` and purpose‑built builders in `src/wallet/`.
- Do: Keep `PATTERN`/`PREFIX` unique and stable for this episode type.
- Do: Prefer `log` macros and structured errors; avoid `println!` in core logic.
- Do: Keep changes scoped to this example unless framework changes are explicitly needed.

### Local Builds Policy (Agents)
- Do not run `cargo build`, `cargo test`, `cargo run`, `cargo fmt`, or `cargo clippy` from agents.
- The user executes cargo locally (WSL/cross‑platform limitations). When verification is required, request the exact command and interpret the output provided by the user.
- Favor static reasoning and minimal diffs; avoid adding or changing rustup/toolchain configuration.

## Key Files
- `src/episode/commands.rs` — Public `ContractCommand` API and `ContractError`.
- `src/episode/contract.rs` — Economic model (rules, bonds, disputes, reputation).
- `src/episode/board_with_contract.rs` — Episode implementation and command handlers.
- `src/episode/handler.rs` — Event handler to publish state back to the UI.
- `src/participant/mod.rs` — Wiring: engine, proxy, generator, CLI workflow.
- `src/wallet/utxo_manager*.rs` — Lock/unlock and micro‑UTXO preparation.
- `src/wallet/kaspa_scripts.rs` — Phase 2.0 script builders (timelock, multisig, combined).
- `docs/` — Security analysis, implementation roadmap, ADRs.

## Extending the Episode Contract
- Add a new command in `src/episode/commands.rs`, then handle it in
  `ContractCommentBoard::execute(...)` with a precise rollback record.
- For state that depends on economic amounts, prefer verifiable inputs tied to the carrier TX.
- When adding moderation or rewards logic, consider how it affects `penalty_pool`,
  `quality_rewards`, and the rollback path.

## On‑Chain Signing and Script‑Enforced Bonds (Roadmap)
Goal: Move from app‑level “economic theatre” to verifiable, script‑enforced locks that the
episode can validate against the exact transaction that carried the command.

Today’s session plan:
1) Verify bond amounts against the carrier TX
   - Short‑term: extend `proxy` → attach minimal TX context (selected outputs: value, script_pubkey,
     and a marker for the episode payload output) into `PayloadMetadata` or a sibling struct passed
     to `Episode::execute`.
   - Episode checks: `assert_eq!(cmd.bond_amount, observed_output_value)` and records `utxo_reference`.
   - Rationale: closes the “payload says 100 KAS but TX carries 1 KAS” gap.

2) Script‑enforced timelock path (single‑party)
   - Default today: P2PK bond output for standardness; episode enforces on‑chain value at submission.
   - Experimental: `--script-bonds` builds a script‑locked output; may be non‑standard until templates stabilize.
   - Next: Use `kaspa-txscript` with finalized templates (timelock/multisig), then verify script descriptor in episode.

3) Moderator escape hatch (multisig)
   - Add combined script (timelock OR N‑of‑M moderator sigs). Store moderator pubkeys in
     `RoomRules`. Validate script template deterministically from rules.
   - Add command for moderator release; episode confirms moderator set and threshold via script match.

4) API and framework touch‑ups (as needed)
   - Engine/proxy: define a minimal, stable TX‑context surface so episodes can verify economic claims
     without separate RPC calls.
   - Generator: no change for wallet ops; keep it for episode messages only.

5) Tests and docs
   - Unit tests for script template builders and episode checks.
   - Happy‑path and failure‑path tests for bond amount mismatch and wrong script template.
   - Update `docs/implementation-roadmap.md` with verification and script milestones.

Optional “Lightning‑style” exploration (future):
- Two‑party conditional bonds (HTLC‑like): add hashlocks and revocation patterns if/when Kaspa
  script set and UX justify it. Keep this example focused on single‑party timelocks + moderator
  release to avoid scope creep.

## Troubleshooting
- Mass/limits: prepare micro‑UTXOs via `UtxoLockManager::ensure_micro_utxos` to avoid oversize TXs.
- Double‑spend/orphan: the participant CLI already retries or guides the user; prefer idempotent
  commands and clear errors.
- Pattern/prefix collisions: keep constants unique; if changing them, update both generator and proxy.

## Conventions
- Rust 2021, `cargo fmt`, `cargo clippy -W warnings`.
- Tests near code with `#[cfg(test)]` as seen in `kaspa_scripts.rs`.
- Prefer explicit types, structured errors, and `log` macros.
