onlyKAS Merchant — Scaffold (M0)

This example sets up the minimal moving parts for an onlyKAS-style merchant flow using kdapp’s Engine/Episode primitives. It mirrors the plan in docs/DEV_WEEKLY_SUMMARY.md (M0: scaffold ReceiptEpisode + handler + SimRouter + TLV + program_id tools).

Scope
- Episode: ReceiptEpisode with commands CreateInvoice, MarkPaid, AckReceipt, CancelInvoice, CreateSubscription, ProcessSubscription, CancelSubscription
- EventHandler: MerchantEventHandler for logging callbacks
- SimRouter: in-process forwarder that wraps EpisodeMessage into EngineMsg::BlkAccepted
- TLV: minimal encoder/decoder for future off-chain transport
- Program ID: derive_program_label helper (placeholder hash of merchant key + label)

Files
- examples/kdapp-merchant/src/episode.rs: ReceiptEpisode state machine
- examples/kdapp-merchant/src/handler.rs: MerchantEventHandler
- examples/kdapp-merchant/src/sim_router.rs: simple EpisodeMessage → Engine wiring
- examples/kdapp-merchant/src/tlv.rs: TLV v1 helpers
- examples/kdapp-merchant/src/program_id.rs: derive_program_label helper
- examples/kdapp-merchant/src/main.rs: demo runner, proxy listener and wiring

Quickstart
- Build: `cargo build -p kdapp-merchant`
- Demo: `cargo run -p kdapp-merchant -- demo`
  - Creates a new episode (merchant key), then CreateInvoice → MarkPaid → AckReceipt
- Proxy listener: `cargo run -p kdapp-merchant -- proxy --merchant-private-key <hex> [--wrpc-url wss://host:port]`

CLI subcommands (M0)
- `demo` — run the default in-process demo.
- `router-udp --bind 127.0.0.1:9530 [--proxy]` — start the UDP TLV router (optionally forwarding via proxy channel).
- `router-tcp --bind 127.0.0.1:9531 [--proxy]` — start the TCP TLV router.
- `proxy [--merchant-private-key <hex>]` — connect to a Kaspa node and stream accepted txs via `kdapp::proxy::run_listener`.
- `new --episode-id <u32> [--merchant-private-key <hex>]` — create episode with merchant pubkey.
- `create --episode-id <u32> --invoice-id <u64> --amount <u64> [--memo <str>] [--merchant-private-key <hex>]` — signed.
- `pay --episode-id <u32> --invoice-id <u64> --payer-public-key <hex>` — unsigned (demo).
- `ack --episode-id <u32> --invoice-id <u64> [--merchant-private-key <hex>]` — signed.
- `cancel --episode-id <u32> --invoice-id <u64>` — unsigned (demo).
- `create-subscription --episode-id <u32> --subscription-id <u64> --customer-public-key <hex> --amount <u64> --interval <u64> [--merchant-private-key <hex>]` — signed.
- `cancel-subscription --episode-id <u32> --subscription-id <u64>` — unsigned (demo).
- `serve --episode-id <u32> --api-key <token> [--bind 127.0.0.1:3000] [--merchant-private-key <hex>]` — start an HTTP server.
- `watch --kaspa-private-key <hex> [--bind 127.0.0.1:9590] [--wrpc-url wss://host:port] [--mainnet]` — anchor checkpoint hashes.
- `register-customer [--customer-private-key <hex>]` — add customer keypair to storage.
- `list-customers` — show registered customer pubkeys and invoice ids.

HTTP server example (uses `X-API-Key` header):

```sh
curl -X POST http://127.0.0.1:3000/invoice \
  -H 'X-API-Key: token' \
  -H 'Content-Type: application/json' \
  -d '{"invoice_id":1,"amount":1000,"memo":"Latte"}'

curl -X POST http://127.0.0.1:3000/pay \
  -H 'X-API-Key: token' \
  -H 'Content-Type: application/json' \
  -d '{"invoice_id":1,"payer_public_key":"<hex>"}'

curl -X POST http://127.0.0.1:3000/subscribe \
  -H 'X-API-Key: token' \
  -H 'Content-Type: application/json' \
  -d '{"subscription_id":1,"customer_public_key":"<hex>","amount":1000,"interval":3600}'

curl -H 'X-API-Key: token' http://127.0.0.1:3000/invoices
curl -H 'X-API-Key: token' http://127.0.0.1:3000/subscriptions
```

Notes
- For signed commands, pass `--merchant-private-key <hex>` so the pubkey matches the episode’s participant list. Otherwise, a fresh keypair is generated for the process which won’t match previous runs.
- The UDP router expects TLV-encoded `EpisodeMessage<ReceiptEpisode>` payloads and forwards them to the engine; a simple sender can be added in M1.
- `router-tcp` provides a reliable TCP alternative for TLV transport with the same forwarding semantics.
- Local state persists in a sled database `merchant.db` with trees for invoices, customers, and subscriptions. Remove the directory to reset or adjust the path in `storage.rs`.

Episode API
- Commands:
  - CreateInvoice { invoice_id, amount, memo }
  - MarkPaid { invoice_id, payer }
  - AckReceipt { invoice_id }
  - CancelInvoice { invoice_id }
  - CreateSubscription { subscription_id, customer, amount, interval }
  - ProcessSubscription { subscription_id }
  - CancelSubscription { subscription_id }
- Rollbacks mirror each action for DAG reorg safety.
- MarkPaid performs coarse validation using tx_outputs in PayloadMetadata when provided by the proxy (>= amount check).

Routing
- Each merchant instance derives a deterministic `PrefixType` and 10-bit `PatternType` from its public key:
  - `prefix = SHA256("onlyKAS:routing" || merchant_pk)[0..4]` (little‑endian u32)
  - `pattern = [(d[4+i], d[14+i] & 1); i=0..9]` where `d` is the same hash
- Override with `--prefix <u32>` and `--pattern "p:b,..."` if needed.
- Off-chain path: use TLV to carry serialized EpisodeMessage; watchers can checkpoint periodically on-chain.

## Checkpoint Protocol
- `MerchantEventHandler` emits `Checkpoint` TLV messages with `{episode_id, seq, state_root}` when invoices are acknowledged
  or at least once every 60 s.
- A lightweight watcher (`watch` subcommand) listens on UDP, verifies the HMAC, and anchors the hash on-chain using an
  `OKCP` record with prefix `KMCP`.
- `seq` is strictly monotone; watchers ignore out‑of‑order checkpoints per `docs/PROGRAM_ID_AND_CHECKPOINTS.md`.

Notes
- This is a scaffold intended for extension: real receipt storage, richer invoice metadata, and actual off-chain transport are deferred to M1+.
- Program ID derivation here is a placeholder; wire to your preferred scheme per docs/PROGRAM_ID_AND_CHECKPOINTS.md.
