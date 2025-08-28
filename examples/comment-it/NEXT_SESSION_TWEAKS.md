# Next Session Goals (comment-it)

- HTTP Subcommand: Wire `http-peer` in `src/main.rs` to dispatch to organizer HTTP server (axum) via `cli/organizer_commands.rs`.
- CLI Flags: Add `--wrpc-url` and `--rpc-retry` (thread through shared client and retry helper; pass into `AuthServerConfig`).
- API Handlers: Apply submit-with-retry to HTTP handler submit paths under `src/api/http/handlers/*` if submitting transactions.
- UTXO Cache (optional): Short-lived cache (e.g., 500–1000 ms) per address to reduce repeated UTXO lookups on rapid submits. [DONE]
- Hardened Challenge Endpoint: Accept optional/malformed JSON, flexible `episode_id` (string/number). [DONE]
- Revoke Session Flow: Remove stored-token check (pure P2P), always emit `session_revoked` WS. [DONE]
- Shared UTXO Cache: Extracted to `kdapp::utils::utxo_cache` and integrated in comment-it. [DONE]
- Docs: Added kaspa-auth model section; remembered session UX; indexing roadmap. [DONE]
- Minimal Indexer Skeleton: New crate `examples/kdapp-indexer` (in-memory store, basic API). [DONE]

Completed This Iteration
- Deterministic Session Handle: Implemented end-to-end. Backend returns handle on WS auth_success and `/auth/status?pubkey=…`; frontend computes as fallback and persists. [DONE]
- Session Restore UX: Restore via `/auth/status?pubkey=…` or indexer membership; hide AuthPanel during restore; unlock comment-form on success. [DONE]
- Engine Rehydrate: On organizer start, rehydrate episodes from kdapp-indexer to avoid "Episode not found" after restarts. [DONE]
- Top Bar UI: Added horizontal top bar (join field + auth indicator + logout); island AuthPanel kept only as fallback. [DONE]
- WS/Indexer Consistency: Unified comment dedup keys; fixed WS timestamp units; brighter author badge; ignore rehydrate `episode_created` for auto-challenge. [DONE]

## Next Session Roadmap
- Indexer Listener: Implemented real listener wiring kdapp proxy+engine, persisting snapshots/memberships/comments. [DONE]
- Optional RocksDB Store: Added `rocksdb-store` feature with on-disk persistence and bincode serialization. [DONE]
- Frontend Remembered Session: Persist `episode_id` + pubkey; restore via indexer `/index/me` and load feed via `/index/*`. [DONE]
- Comment-It Bonds: Design optional anti-spam bond (inspired by comment-board) and wire minimal UI hints.
- Reusable Endpoints: Added `GET /index/me/{episode_id}` and `/index/metrics`. [DONE]
- Rollup APIs: Pagination for `/index/comments/:id?after&limit`, `/index/recent?limit` integrated into UI. [DONE]
- Docs Hygiene: Added deterministic handle design doc; plan to link from README.

## New Next Session Roadmap
- Deterministic Handle: Expose handle via a small `/auth/handle/{episode_id}?pubkey=…` endpoint and/or `/index/members/{episode_id}`. [PLANNED]
- Engine Rehydrate: Completed as above. [DONE]
- Single-Tab Guard: Add BroadcastChannel leader election to prevent duplicate WS/polling across tabs.
- CLI/Env: Add flags for kdapp-indexer (wrpc-url, network) and optional runtime storage selection; today uses `INDEXER_URL`. [PARTIAL]
- Kaspax Patch: Prepare patches/PKGBUILD for kdapp-indexer (rocksdb-store) on Linux, add systemd unit sample.
- Docs: Link deterministic handle doc from README; add "What kaspa-auth is/isn't" section; describe top bar UX. [PLANNED]
- Top Bar Search: Add simple indexer-backed free-text search (by author/text) to top bar. [PLANNED]
- Submit Gate (optional): Disable submit button until authenticated; show inline hint; backend remains source of truth. [PLANNED]

## Notes
- WS peer remains as a lightweight runner.
- HTTP organizer peer provides the UI + richer flows described in TESTING.md.
