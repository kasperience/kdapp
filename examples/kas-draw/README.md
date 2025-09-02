kas-draw (M1 MVP)

Lean lottery Episode demo on Kaspa L1 using kdapp.

- Scope: one ticket → one draw → one claim (single participant key)
- Transport: L1 payload-carrying txs (onlyKAS optional later)
- TLV v1 & state_hash defined to avoid early bikeshedding

TLV v1 (onlyKAS, deferred to M4)

- version: 0x01
- fields (little-endian):
  - type: u8 (0=New,1=Cmd,2=Ack,3=Close)
  - episode_id: u64
  - seq: u64
  - state_hash: [u8;32]
  - payload_len: u16 + payload: [u8] (borsh)
- See `examples/kas-draw/src/tlv.rs` and `hash_state()` (BLAKE2b-512 truncated to 32B).

Threat Model (M1)

- Stale-state close (onlyKAS): guarded later via watchtowers (CSV window).
- Replay/order: strict seq + state_hash (onlyKAS path).
- Entropy griefing: cap draw frequency; mix block metadata + user salt.
- DoS via bulk buys: rate-limit; minimum ticket value. (M2+)

CLI (dev-only)

- new --episode-id <id>
- buy --episode-id <id> --amount <atoms> n1 n2 n3 n4 n5
- draw --episode-id <id> --entropy <str>
- claim --episode-id <id> --ticket-id <u64> --round <u64>

Runner (L1 end-to-end)

- engine --mainnet/--no-mainnet [--wrpc-url wss://…]  – starts engine + listener (Ctrl+C to exit)
- submit-new --episode-id <id> --kaspa-private-key <hex> --mainnet/--no-mainnet [--wrpc-url …]
- submit-buy --episode-id <id> --kaspa-private-key <hex> --amount <atoms> n1 n2 n3 n4 n5 --mainnet/--no-mainnet [--wrpc-url …]
- submit-draw --episode-id <id> --kaspa-private-key <hex> --entropy <str> --mainnet/--no-mainnet [--wrpc-url …]
- submit-claim --episode-id <id> --kaspa-private-key <hex> --ticket-id <u64> --round <u64> --mainnet/--no-mainnet [--wrpc-url …]

Notes

- Use testnet-10 by default (omit --mainnet). Fund the address derived from your private key with at least `amount + fee` (fee default: 5_000 sompi).
- BuyTicket enforces `entry_amount == ticket_price` and, if proxy provides `tx_outputs`, at least one output value ≥ `ticket_price` (M1 relaxed).
- Draw interval is short for demos (15s). If you submit draw too early, engine logs a rejection; wait a few seconds and try again.
- Auto wRPC URL resolution: omit `--wrpc-url` to use the default for your network via resolver. You can also set `WRPC_URL` env var to override, e.g. `WRPC_URL=wss://host:port cargo run -p kas-draw -- engine`.
- Dev key convenience (off-chain send): if you don't pass `--kaspa-private-key`, the CLI will try `KASPA_PRIVATE_KEY`, then `KAS_DRAW_DEV_SK`, then a dev key file at `examples/kas-draw/dev.key` (gitignored). Put a testnet-only dev key hex in that file to auto-authorize NEW/BUY in local demos.
 - Routing constants are centralized under `src/routing.rs`. `PREFIX` identifies the episode family and the 10‑bit `PATTERN` is derived deterministically from the prefix to avoid copy‑paste drift across modules. External tools should import from `routing` instead of duplicating values.
 - For new contributors, you may set `KAS_DRAW_USE_TEST_KEY=1` to use a deterministic test key for local demos. Never use this on mainnet; prefer explicit keys via flag/env for clarity.

Watchtower flow

- The included watchtower simulator (`watchtower::SimTower`) captures state roots emitted by the handler and illustrates how off‑chain checkpoints could be collected and later anchored on-chain.
- See `src/handler.rs` for where state roots are computed and relayed, and `src/tlv.rs` for the TLV layout. The `submit_checkpoint` subcommand demonstrates posting roots using the `CHECKPOINT_PREFIX`.
- This models the onlyKAS “off‑chain checkpoint + on‑chain anchor” philosophy; the simulator can be replaced with a real watcher once finalized.

Demo Clip Criteria (M1)

- 30s asciinema: New → Buy → Draw → Claim; logs show state transitions.

Next (M2)

- Tiered prize math, rollover, EmergencyPause, signed UpdatePrizeDistribution.
- UTXO policy: mirror patterns from examples/comment-board (locking windows & claim checks).

L1 Enforcement (M1 note)

- When `tx_outputs` are provided by the proxy, `BuyTicket` enforces that at least one carrier transaction output equals `ticket_price`. This keeps the MVP simple without requiring script bytes. In M2, tighten by enforcing an escrow address/script and claim windows similar to comment-board.

Autopilot Scripts (Dev Convenience)

- Off‑chain demo: `examples/kas-draw/offchain_demo.ps1`
  - Starts off‑chain engine + UDP router and sends NEW → BUY → wait → DRAW → CLOSE.
  - Options: `-EpisodeId`, `-Key`, `-Bind`, `-Amount`, `-Numbers`, `-WaitDrawSeconds`, `-UseBin`, `-NoStartEngine`.
  - Key fallback if `-Key` is omitted: env `KASPA_PRIVATE_KEY`, env `KAS_DRAW_DEV_SK`, then file `examples/kas-draw/dev.key`.

- On‑chain demo: `examples/kas-draw/onchain_demo.ps1`
  - Starts L1 engine (wRPC) and submits NEW → BUY → wait → DRAW on testnet‑10 by default.
  - Options: `-EpisodeId`, `-Key` (same fallback), `-WrpcUrl` or env `WRPC_URL`, `-Mainnet`, `-Amount`, `-Numbers`, `-WaitDrawSeconds`, `-UseBin`, `-NoStartEngine`.

- TUI: dashboard renders immediately with a teal banner and shows Mechanics (range, ticket price, draw interval, ETA) plus recent events.

What’s Next (Project Ideas)

- Provable raffle (commit‑reveal): buyers commit `H(numbers||salt)`; draw uses a predetermined block hash; winners reveal to claim.
- Escrowed auction/bids: lock funds on L1 (UTXO), run price discovery off‑chain, and settle on‑chain with timelocked refund paths.
- Red packet drops: sponsor funds an address; off‑chain claims race against time; L1 script enforces expiry and refunds.
- Proof‑of‑attendance stamps: off‑chain signatures with periodic on‑chain checkpoints (indexable via kdapp‑indexer).
- Micro‑paywall/access tickets: onlyKAS channel for access; periodic checkpoints anchor “who paid” without a VM.

Program‑ID + Checkpoints (RFC preview)

- Code anchor: compute `PROGRAM_ID = BLAKE2b(canonical_source_bundle)` and publish once (genesis) to tie episodes to code.
- State commitments: each step computes `STATE_ROOT = BLAKE2b(Borsh(state))`; watchers verify transitions using the anchored code.
- Periodic checkpoints: tiny L1 payload `(episode_id, seq, state_root)` for public auditability without executing the program on L1.
