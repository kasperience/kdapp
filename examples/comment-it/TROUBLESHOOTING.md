# comment-it Troubleshooting & Quick Checks

This page summarizes how to verify the app after a restart, what the UI states mean, and what to try if a step seems stuck.

## Quick Start (after restart)
- Start kdapp-indexer (RocksDB):
  - PowerShell: `$env:INDEX_DB_PATH=".kdapp-indexer-db"; cargo run -p kdapp-indexer --features rocksdb-store`
- Start HTTP peer with indexer integration:
  - PowerShell: `$env:INDEXER_URL="http://127.0.0.1:8090"; cargo run -p comment-it -- http-peer --port 8080`
- Open `http://localhost:8080`.

Expected logs on the HTTP peer:
- `Rehydrated N episode(s) from kdapp-indexer`
- For your room: `Auth episode <id> initialized on blockchain` followed by `Stored episode <id> in blockchain state`

## UI States
- Top bar `Auth: guest`: UI is showing the comment form; backend still enforces authorization. A silent re‑auth may be running or will start on your first action.
- Top bar `Auth: authenticated`: On‑chain membership is active; comments are accepted immediately.
- Island AuthPanel: Fallback only. If it appears after reload, a restore attempt failed (see below).

## Verify Auth Status
- Browser/cURL: `GET /auth/status/{episode_id}?pubkey={pubkey_hex}` (no braces)
  - Example: `http://localhost:8080/auth/status/511466637?pubkey=027e28...67ba`
  - `authenticated: true` → good. `pending` → silent re‑auth may still be running or is needed.

## Common Issues & Fixes

1) Feed loads but submit is rejected (`Participant not authenticated`)
- Cause: Engine lost in‑memory membership on restart. We now rehydrate episodes and (for `/api/comments/simple`) populate membership in memory if kdapp-indexer reports `member: true`.
- Check: Submit a comment; if logs still show rejection, click Authenticate to run the challenge flow. Status should flip to `authenticated: true` quickly afterwards.

2) `Episode <id> not found` on submit after restart
- Cause: Previously, rehydrate used a zero DAA and episodes were purged by the lifetime filter. Fixed: rehydrate uses current DAA.
- Check startup logs for `Rehydrated ...` and ensure the warning no longer appears on submit.

3) AuthPanel flashes on hard reload (Ctrl+F5)
- Fixed by deferring the panel until restore completes. If you still see it, ensure `INDEXER_URL` is set and kdapp-indexer is up.

4) Wrong or placeholder wallet address in the status bar
- The status bar now updates as soon as the wallet is loaded, even if the AuthPanel is deferred. If you see `kaspa:qrxx...v8wz`, reload once to pick up an existing wallet.

5) Duplicate comments or wrong times
- WS and indexer dedup keys unified; timestamps normalized to local date/time.

## Notes
- The deterministic session handle is returned by backend status and can be computed locally; it acts like a cookie for unlock‑until‑revocation. The chain still enforces authorization.
- You can always force the full challenge/verify from the UI to refresh on‑chain membership.

