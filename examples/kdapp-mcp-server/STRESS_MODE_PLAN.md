# Tic‑Tac‑Toe Stress Mode (6‑Move Cap) — Implementation Plan

## Goal
Add an opt‑in “stress mode” that mirrors Michael’s 6‑move sliding‑window variant to:
- Increase agent planning difficulty (anticipate eviction before moving).
- Exercise engine rollback semantics (evicted cell restoration on reorgs).
- Keep classic (9‑move, no eviction) mode as default.

## Current Status
- Engine toggle implemented (env‑driven): examples/kdapp-mcp-server/src/state.rs
  - `KDAPP_TTT_STRESS=1` or `KDAPP_TTT_MAX_SYMBOLS=<n>` enables cap with eviction.
  - Maintains `move_history`, evicts on cap, records `removed` in rollback, restores on rollback.
- Coordinator currently uses classic prompts; no stress‑aware prompts yet.

## Deliverables
- Stress‑aware strategy context tool for agents (`kdapp_get_strategy_context`).
- Enriched snapshots to expose cap, recent moves, and “next_to_remove”.
- Coordinator prompt upgrades gated by stress mode.
- Minimal docs and usage examples.

## Design
- Modes
  - Classic (default): 9‑move, no eviction.
  - Stress (opt‑in): cap=N (default 6) with oldest‑first eviction before applying the new move.
- Toggle
  - Env variables:
    - `KDAPP_TTT_STRESS=1|true|yes` → cap=6
    - `KDAPP_TTT_MAX_SYMBOLS=<n>` (n>0) → cap=n

## Engine Changes (state.rs)
- Already implemented:
  - `move_history: VecDeque<(u8,u8)>` (only populated when stress mode is ON).
  - Execute: if `len()==cap` → pop_front, clear board cell, save `TttRemoved {row,col,symbol}`; then apply move, push_back.
  - Rollback: remove last move, pop_back; if `removed` present, restore cell and push_front to preserve order.
- To add:
  - Enrich `TttSnapshot` with:
    - `cap: Option<u8>`
    - `recent_moves: Option<Vec<(u8,u8,u8)>>` (row,col,player_code)
    - `next_to_remove: Option<(u8,u8,u8)>` (front of history when len==cap)
  - Update `TttEventHandler::{on_initialize,on_command,on_rollback}` to populate these Option fields when stress mode is ON.

## New MCP Tool: kdapp_get_strategy_context
- Purpose: Give agents pre‑move context so they can plan with the imminent eviction in mind.
- Request (JSON):
  - `{"name":"kdapp_get_strategy_context", "arguments":{"episode_id":"<str>"}}`
- Response (JSON):
  - `board`: `[[u8;3];3]` (0 empty, 1 X, 2 O)
  - `current`: `'X'|'O'`
  - `cap`: `number|null`
  - `next_to_remove`: `{row:u8,col:u8,player:'X'|'O'}|null`
  - `recent_moves`: `[{row,col,player:'X'|'O'}...]` oldest→newest or `[]`
  - `rule_text`: short string describing the active rule
- Implementation:
  - `rpc_handlers.rs`: register tool in `handle_tools_list` and route in `handle_tools_call`.
  - `tools.rs`: new function reads `state.ttt_state` to build response. Map numeric player codes to `X|O`.

## Coordinator Prompt Upgrades (tictactoe_coordinator.py)
- Behavior:
  - Read stress toggle from env (`KDAPP_TTT_STRESS` or `KDAPP_TTT_MAX_SYMBOLS`).
  - When stress ON:
    - Either call `kdapp_get_strategy_context` each turn or maintain a local `move_history` mirror.
    - Inject into prompts:
      - Rule text: “Board shows at most N symbols; before your move, the oldest symbol is removed.”
      - `Recent moves (oldest→newest)`.
      - `Will be removed next: (row,col,player)` when len==cap.
  - When stress OFF: keep current classic prompts.

## Testing Plan
- Unit Tests (Rust):
  - Execute/rollback:
    - With cap=6: after 7th move, oldest cell cleared; rollback restores evicted cell and order.
    - With classic mode: no eviction; rollback clears last move only.
  - Invariants: `move_history.len() <= cap`, board matches history, `current` switches consistently.
- Integration (manual or scripted):
  - Run with env toggles; verify engine logs, JSON snapshots, and coordinator prompt text.
  - Simulate reorg (engine rollback message) and validate state restoration in both modes.

## Docs & Examples
- README update (kdapp-mcp-server):
  - Add “Stress Mode” section with env toggles and expected behavior.
  - Show example response of `kdapp_get_strategy_context`.
- Quickstart snippet:
  - PowerShell: `$env:KDAPP_TTT_STRESS='1'; cargo run -p kdapp-mcp-server`
  - Linux/macOS: `KDAPP_TTT_STRESS=1 cargo run -p kdapp-mcp-server`

## Rollout & Compatibility
- Defaults remain classic; no behavior change unless env is set.
- Snapshot additions are optional fields; existing consumers won’t break.
- No schema/version bump needed for internal demo usage.

## Risks & Mitigations
- Drift between engine and coordinator histories → prefer fetching `kdapp_get_strategy_context` each turn.
- Agent overfitting prompts → keep context concise and structured.

## Timeline (est.)
- Snapshot enrichment: 0.5 day
- MCP tool + handlers: 0.5 day
- Coordinator prompts + optional fetch path: 0.5 day
- Tests + docs polish: 0.5 day

---
Owner: Michael Sutton (concept), implementation support tomorrow.
