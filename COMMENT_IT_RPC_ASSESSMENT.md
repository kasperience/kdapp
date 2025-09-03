# comment-it RPC/State Flow Assessment (dev branch)

This report summarizes how `examples/comment-it` handles RPC and state today on `dev`, how it differs from `comment-board`, and the minimum changes to harden it similarly.

## Entry Points
- `src/main.rs`
  - Starts a WebSocket server for a frontend.
  - For each frontend message (SubmitComment, RequestChallenge, SubmitResponse, RevokeSession):
    - Builds `EpisodeMessage` using `kdapp::episode::EpisodeMessage`.
    - Calls `kdapp::proxy::connect_client(network, None)` per request.
    - Fetches UTXOs for the signer address.
    - Builds a tx and calls `submit_transaction` once (no retry/reconnect/orphan handling).
- `src/episode_runner.rs`
  - Runs kdapp engine with `AuthWithCommentsEpisode` via `run_auth_server`.
  - Uses `kdapp::proxy::run_listener` to pull accepted txs and emit events to an HTTP endpoint or event channel.
  - This leverages the global listener (now resilient in kdapp) for state updates.

## What’s Already Solid
- Listener path: `run_listener` comes from `kdapp::proxy`, which on `dev` was improved to auto‑reconnect on WebSocket drops and reset the sink, keeping state streaming after transient failures.

## High‑Risk Spots in comment‑it
- Per‑request client creation in `main.rs`:
  - Every frontend command opens a new `KaspaRpcClient` and immediately submits a tx.
  - No retry logic on errors like “WebSocket disconnected/not connected”.
  - No reuse of a warm client; repeated connects can amplify latency and failure probability.
- No orphan handling:
  - `submit_transaction` errors are returned to the frontend without retrying on “orphan” or refreshing UTXOs first (unlike comment‑board’s wallet path).

## Recommendations (Minimal Changes)
- Reuse a single RPC client (or small pool):
  - Initialize one `KaspaRpcClient` at startup with `connect_client`, and share it across requests.
  - Benefit: avoids repetitive connect storms and leverages reconnect policy.
- Add retry‑on‑disconnect for submits:
  - Port a small helper (like comment‑board’s `submit_tx_retry`) to retry when errors contain “WebSocket”, “not connected”, or “disconnected”. Treat “already accepted” as success.
- Handle orphan gracefully:
  - On orphan, refresh UTXOs and retry once.
- Optional: short‑lived UTXO cache per address to lower request latency, or fetch on demand + 1 retry.

## Minimal Touch Points
- `src/main.rs`:
  - Create and share a `KaspaRpcClient` at startup.
  - Add `submit_with_retry(kaspad, tx)` similar to comment‑board.
  - Replace direct `.submit_transaction(..)` calls with the retry helper.
- `src/episode_runner.rs`:
  - Listener already benefits from kdapp’s reconnect fixes; no immediate change required.

## Suggested Implementation Plan
1. Add a module `rpc.rs` with:
   - `get_shared_client(network, url) -> Arc<KaspaRpcClient>`
   - `submit_with_retry(kaspad, tx, attempts)` mirroring comment‑board logic.
2. In `main.rs`, construct the shared client once and use it in all handlers.
3. Add one orphan‑refresh retry before surfacing errors.
4. Optionally, add CLI flags for retry count in a later pass.

## Branch Diff Summary (master..dev)
These files differ under `examples/comment-it`:
- M src/api/http/organizer_peer.rs
- M src/cli/commands/demo.rs
- M src/cli/utils.rs
- M src/comment.rs
- M src/core/episode.rs
- M src/organizer.rs

Note: The core RPC submission paths live in `src/main.rs` (frontend server) and are currently per‑request connects with no retries.

---
Prepared to guide a precise patch when you want changes applied on `dev`.
