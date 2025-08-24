# Next Session Goals (comment-it)

- HTTP Subcommand: Wire `http-peer` in `src/main.rs` to dispatch to organizer HTTP server (axum) via `cli/organizer_commands.rs`.
- CLI Flags: Add `--wrpc-url` and `--rpc-retry` (thread through shared client and retry helper; pass into `AuthServerConfig`).
- API Handlers: Apply submit-with-retry to HTTP handler submit paths under `src/api/http/handlers/*` if submitting transactions.
- UTXO Cache (optional): Short-lived cache (e.g., 500–1000 ms) per address to reduce repeated UTXO lookups on rapid submits.
- Docs Hygiene: Add banners to legacy docs to point to README; consider pruning duplicates.

## Notes
- WS peer remains as a lightweight runner.
- HTTP organizer peer provides the UI + richer flows described in TESTING.md.
