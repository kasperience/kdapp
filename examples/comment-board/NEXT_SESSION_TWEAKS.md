# Next Session Tweaks

This file tracks targeted improvements to pursue next, based on recent testing.

## RPC & Listener
- Shorter post-activity cadence: poll `get_virtual_chain_from_block` every 250–300 ms for ~5 s after a relevant tx, then decay to 1 s.
- Reconnect backoff: exponential (250 ms → 2 s) with jitter; cap at ~5 s.
- CLI knobs: `--rpc-retry <n>` (default 3), `--listener-interval-ms <ms>` (default 1000), `--listener-burst-ms <ms>` (default 300), `--listener-burst-window <ms>` (default 5000).
- Consider subscription API if/when available in `kaspa-wrpc-client` to replace polling.

## Submit Flow
- Orphan handling: bounded retries with small delay (e.g., 200–400 ms) and one forced UTXO refresh; better logging of orphan root cause.
- Treat “already accepted” as success (done); add metrics counters for retries.

## Wallet & UTXO Prep
- Add `--no-micro-utxos` to skip auto-split on public nodes that reject mass even for empty-payload txs.
- Make split target configurable: `--split-chunk <sompis>` (default 50_000), `--split-threshold <sompis>` (default 100_000).

## UI/UX
- Optional compact render: only print diffs (new comments) to reduce scroll.
- Timestamp formatting option: human-readable vs raw millis.
- Show live connection status (Connected/Reconnecting) in the header.

## Engine/State
- Ensure immediate handler dispatch without batching; verify no internal debounce.
- Consider emitting a local echo upon submit, then reconcile on next state to further reduce perceived latency.

## Testing
- Add a harness to simulate WebSocket drop/reconnect and orphan acceptance sequences.
- Unit tests for retry helper paths and listener reconnect behavior.

## Documentation
- Expand README with CLI flags once added; include troubleshooting tips for PNN instability and faucet links.

