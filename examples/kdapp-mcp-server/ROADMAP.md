# kdapp MCP Server — Next Steps Roadmap

Goals: Keep this example production-honest (no mock state), easy to extend, and contributor-friendly. Below is a curated set of next steps for anyone interested in picking up work.

Core Improvements
- Participant AuthZ: Add `kdapp_register_participants` tool to register arbitrary pubkeys for X/O; validate signer pubkey against registered participants (currently tied to server agent wallets).
- Full Recovery: Rebuild engine state on restart by loading snapshots/events into the engine (not only preloading for read). Option A: replay compact event log; Option B: persist minimal state + rollback stack.
- Indexer Tools: Expose MCP tools to query `episodes/events.jsonl` by episode, time range, or outcome; optionally add an HTTP endpoint behind a feature flag.
- On-chain Strict Mode: When node is available + wallets funded, require successful tx submission before accepting a move (configurable flag).

Developer Experience
- main.rs Size Budget: Keep under 50 LOC by routing startup/loop to `app.rs` and JSON-RPC routing to `rpc_handlers.rs` (done).
- Structured Logging: Add a minimal logger wrapper with levels + color, guard JSON-RPC lines (stdout) vs logs (stderr) for clean MCP streams.
- Tests: Add unit tests for `TicTacToeEpisode::execute/rollback`, and lightweight integration test for tool routing.

Security/Validation
- Signature Paths: Support external signed moves (signature + pubkey args) in addition to wallet-signed path; verify signature over serialized command bytes.
- Replay Protection: Add nonce/sequence per episode to prevent duplicate moves.
- Input Validation: Strengthen command schema for `row`, `col`, and `player` with precise types and ranges.

UX Enhancements
- Explorer Links: Return `{ txid, explorer_url }` on successful submissions (done).
- Colorized Output: Differentiate Agent 1 (teal) and Agent 2 (orange) in coordinator (done).
- Health Probe: Add `kdapp_health` tool to verify node connectivity and wallet funding status.

Kaspa Auth Integration
- Identity Flow: Integrate `examples/kaspa-auth` 3‑stage flow to bind identities to participant pubkeys.
- Session Tokens: Cache authenticated sessions and enforce expiry; surface failure reasons via MCP.

Stretch Ideas
- Tournament Mode: Orchestrate series of games and persist scoreboards.
- Web Dashboard: Expose read-only snapshots + recent events feed.
- Alternate Episodes: Add a simpler episode (e.g., Counter) and a richer one (e.g., Connect Four) to showcase engine flexibility.

How to Contribute
- Pick a checkbox above and open a PR with a short design note.
- Follow repo guidelines: `cargo fmt`, `cargo clippy -W warnings`, and `cargo test --workspace`.

