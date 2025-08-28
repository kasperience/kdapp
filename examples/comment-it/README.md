# comment-it (Pure kdapp Peer)

A kdapp example that exposes a WebSocket API to a frontend and submits signed Episode commands to Kaspa while a local kdapp engine listens for matching transactions and emits application state.

## Features
- WebSocket server for frontend → forwards commands as signed `EpisodeMessage`s.
- Shared kdapp engine + listener (pattern/prefix routing) for state updates.
- Uses the kdapp `TransactionGenerator` to build payload transactions.

## Run

Mode A — WebSocket peer (lightweight)
- Start: `cargo run -p comment-it -- --ws-addr 127.0.0.1:8080`
- Client: connect to `ws://127.0.0.1:8080` and send JSON commands:
  - `{ "SubmitComment": { "text": "hi", "episode_id": 123 } }`
  - `{ "RequestChallenge": { "episode_id": 123 } }`
  - `{ "SubmitResponse": { "episode_id": 123, "signature": "...", "nonce": "..." } }`
  - `{ "RevokeSession": { "episode_id": 123, "signature": "..." } }`

Mode B — HTTP organizer peer (full UI)
- Start: `cargo run -p comment-it -- http-peer --port 8080`
- Open: `http://localhost:8080` (wallet flows + auth + comments)
- CLI helpers: see `src/cli/organizer_commands.rs` and `TESTING.md`

## RPC Reliability (dev)
- Reuses a shared Kaspa RPC client instance.
- Retry-on-disconnect for `submit_transaction` (treats "already accepted" as success; retries on transient WebSocket errors and orphan cases).
- Listener auto-reconnects and resets sink via kdapp core.

## Notes
- Uses testnet-10 by default; provide a `--wrpc-url` in engine/runner if you need a stable node.
- See `COMMENT_IT_RPC_ASSESSMENT.md` for deeper analysis and planned follow-ups.

Tips
- Indexer URL: set `INDEXER_URL` (default `http://127.0.0.1:8090`) so `/auth/status/:id` can fall back to kdapp-indexer membership on restart and avoid re‑authentication prompts.

## Top Bar UX (Join + Auth)
- Join: Use the top bar room field to join an existing episode by ID; the feed loads immediately from the indexer.
- Auth indicator: Shows `guest` until authorization is restored (or completed), then `authenticated`. Use the top‑right Logout button to revoke.
- AuthPanel fallback: The island AuthPanel only appears if restore fails or no wallet exists; under normal reloads you won’t see it.

## Deterministic Session Handle
- Purpose: A stable, episode‑scoped “cookie” computed from `(episode_id, pubkey)` to restore UI state across restarts without a centralized server.
- Backend: Returns `session_token` on WS `authentication_successful` and on `GET /auth/status/{episode_id}?pubkey=…`.
- Frontend: Persists `last_episode_id`, `participant_pubkey`, and the handle; if needed, computes the same handle locally and unlocks the comment form while the chain/indexer enforce auth on submit.

## Session Restore Flow (Hard Reload)
1) Browser loads the feed for the last room from kdapp‑indexer.
2) UI defers the island AuthPanel and calls `GET /auth/status/{episode_id}?pubkey=…`.
3) If authenticated (or indexer membership): UI sets the session handle and shows the comment form immediately.
4) If not authenticated yet: UI starts a silent re‑auth in the background; the AuthPanel stays hidden unless re‑auth fails.

## Engine Rehydrate on Start
- The HTTP peer rehydrates recent episodes from kdapp‑indexer at startup to avoid dropping commands with “Episode not found” after restarts.
- Look for `Rehydrated N episode(s) from kdapp-indexer` in logs.

## Configuration
- `INDEXER_URL`: Base URL for kdapp‑indexer used by the organizer peer (default `http://127.0.0.1:8090`).
- `INDEX_DB_PATH`: Path for kdapp‑indexer RocksDB (default `.kdapp-indexer-db`).
- Network: Testnet‑10 by default; override node URL in engine/runner as needed.

Run kdapp‑indexer (RocksDB):
- PowerShell: `$env:INDEX_DB_PATH=".kdapp-indexer-db"; cargo run -p kdapp-indexer --features rocksdb-store`

Run HTTP peer with indexer integration:
- PowerShell: `$env:INDEXER_URL="http://127.0.0.1:8090"; cargo run -p comment-it -- http-peer --port 8080`

## Troubleshooting
- AuthPanel after reload: Ensure `participant_pubkey` is available and `GET /auth/status/{id}?pubkey=…` returns `authenticated: true`; verify `INDEXER_URL` is set and the indexer is running.
- Episode not found on submit: Confirm the startup log shows rehydrate; verify kdapp‑indexer has the episode in `/index/recent` and that the organizer logs “Stored episode … in blockchain state”.
- Duplicate comments: Hard refresh to load the latest JS; WS and indexer dedup keys are unified.
- Wrong time: WS now uses millisecond timestamps; both WS and indexer display local date/time consistently.

See also: `TROUBLESHOOTING.md` for deeper checks, status meanings, and common fixes.

## Kaspa‑Auth Model (Quick Guide)
- Episode‑scoped authorization enforced by on‑chain signed commands.
- No centralized cookie/session; the HTTP peer is a UX bridge.
- The “session token” is a capability handle within an episode and is revocable on‑chain.

## Remembering Sessions (Recommended UX)
- Persist `episode_id` and your pubkey in `localStorage`.
- On page load: call `GET /auth/status/{episode_id}` and subscribe to WebSocket.
- If authenticated, restore authenticated UI immediately and skip the challenge flow.
- On `session_revoked`, clear only the authenticated flag (optionally keep last `episode_id`).

## Roadmap: Optional Bonds & Indexing
- Optional anti‑spam bonds (like in comment‑board) can be added to require a small stake per room.
- A lightweight indexer can persist episode snapshots and provide fast boot APIs for quick room discovery and state restore.
