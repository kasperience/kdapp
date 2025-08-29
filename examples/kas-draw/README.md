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

Demo Clip Criteria (M1)

- 30s asciinema: New → Buy → Draw → Claim; logs show state transitions.

Next (M2)

- Tiered prize math, rollover, EmergencyPause, signed UpdatePrizeDistribution.
- UTXO policy: mirror patterns from examples/comment-board (locking windows & claim checks).

L1 Enforcement (M1 note)

- When `tx_outputs` are provided by the proxy, `BuyTicket` enforces that at least one carrier transaction output equals `ticket_price`. This keeps the MVP simple without requiring script bytes. In M2, tighten by enforcing an escrow address/script and claim windows similar to comment-board.
