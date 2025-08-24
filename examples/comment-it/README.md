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
