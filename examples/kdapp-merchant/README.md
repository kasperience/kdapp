# kdapp-merchant

This example demonstrates a simple merchant application built on kdapp. It includes an optional checkpoint watcher that anchors state hashes to the Kaspa network.

Key protocol notes
- TLV transport includes `Handshake`, `New`, `Cmd`, `Ack`, `Close`, `AckClose`, `Checkpoint`, and `Refund` types.
- Routers (UDP/TCP) enforce a per-peer handshake and HMAC, forward `New/Cmd/Close/Checkpoint` to the engine, and ignore `Ack/AckClose/Refund`.
- The watcher validates `Checkpoint` messages (HMAC) before anchoring and separately accepts `Refund` messages with guardian signatures, verifying `(tx, sig, gpk)`.
- `PubKey` implements `Hash`, so it can be used in `HashMap`/`HashSet` (e.g., guardian handshake tracking).

## Watcher Configuration

The watcher fee and congestion behaviour can be tuned using two parameters:

- `max_fee` – maximum fee (in sompis) for an anchoring transaction. The watcher defers if the estimated fee exceeds this limit. **Default:** no limit. **Recommended range:** 5,000 – 100,000.
- `congestion_threshold` – mempool congestion ratio above which anchoring is deferred. **Default:** 0.7. **Recommended range:** 0.5 – 0.9.

### Via CLI

Provide these options when starting either the `serve` or `watcher` subcommands:

```bash
kdapp-merchant serve --max-fee 50000 --congestion-threshold 0.8
kdapp-merchant watcher --max-fee 50000 --congestion-threshold 0.8
```

### Via HTTP

When running the `serve` subcommand, the watcher settings can be updated at runtime:

```http
POST /watcher-config
Content-Type: application/json
x-api-key: <API_KEY>

{
  "max_fee": 50000,
  "congestion_threshold": 0.8
}
```

The provided values apply to the currently running watcher process.

## Mempool Metrics

The watcher tracks the most recent fee estimate and a simple mempool congestion ratio. These metrics help decide when to anchor
checkpoints.

### Via CLI

```bash
kdapp-merchant watcher --show-metrics
```

Outputs the current `base_fee` (sompis required for a small transaction) and `congestion` ratio.

### Via HTTP (server)

```http
GET /mempool-metrics

{
  "base_fee": 5000,
  "congestion": 0.42
}
```

- `base_fee` – conservative fee in sompis for anchoring transactions.
- `congestion` – mempool size ratio (higher values indicate a busier mempool).

### Via HTTP (watcher)

If you start the watcher with `--http-port <port>`, it exposes:

```http
GET /mempool

{
  "est_base_fee": 5000,
  "congestion_ratio": 0.42,
  "min": 1000,
  "max": 10000,
  "policy": "default"
}
```

## Dispute & Refund Flow (testnet)

End-to-end steps to reproduce pay-per-invoice, dispute, and guardian refund.

1) Start the checkpoint watcher (anchors to testnet-10):

```sh
cargo run -p kdapp-merchant -- watcher \
  --kaspa-private-key <hex> \
  --wrpc-url wss://node:16110 \
  --http-port 9591
```

2) Start the guardian service and copy its public key from logs:

```sh
cargo run -p kdapp-guardian --bin guardian-service -- --config examples/kdapp-guardian/config.toml
```

3) Create an episode and invoice, supplying the guardian address/key so disputes reach the guardian:

```sh
# create episode 42 and an open invoice 1
cargo run -p kdapp-merchant -- \
  --guardian-addr 127.0.0.1:9650 \
  --guardian-key <guardian_pubkey_hex> \
  new --episode-id 42

cargo run -p kdapp-merchant -- \
  --guardian-addr 127.0.0.1:9650 \
  --guardian-key <guardian_pubkey_hex> \
  create --episode-id 42 --invoice-id 1 --amount 1000 --memo test
```

4) Open a dispute by canceling before payment (demo policy):

```sh
cargo run -p kdapp-merchant -- cancel --episode-id 42 --invoice-id 1
```

This triggers an `Escalate` to the guardian. The guardian co‑signs a refund and notifies the watcher; in watcher logs expect:

- `checkpoint received: ep=42 seq=...` (periodic)
- `refund verified for ep=42 seq=0`

In guardian logs: `guardian: co-signed refund and notified watcher for episode 42`.

### Trusting guardian signatures

- Verification: the watcher checks `verify_signature(guardian_pubkey, hash(refund_tx), guardian_sig)` before logging.
- You can also verify locally via `kdapp::pki::verify_signature` if you capture the `(tx, sig, gpk)` tuple.

Notes:
- The demo uses a shared HMAC key (`kdapp-demo-secret`) for TLV; change this in production.
- Refund TLV is consumed by the watcher only; routers ignore it by design (see onlyKAS-merchant.md).

## Troubleshooting

- anchoring deferred: congestion above threshold
  - The watcher skipped on-chain anchoring due to mempool load. Lower `--congestion-threshold`, raise `--max-fee`, or retry later.
- anchoring deferred: fee {fee} exceeds max {max_fee}
  - Increase `--max-fee` or adjust at runtime via `POST /watcher-config`.
- wrpc connect errors / timeouts
  - Verify `--wrpc-url` and network flag (`--mainnet` vs testnet-10 default). The watcher reconnects automatically on transient disconnects.
- guardian: escalation for unknown episode
  - The guardian hasn’t seen checkpoints for the episode yet. Once it observes OKCP anchors (or receives another escalate), the dispute will be tracked.
- ack timeouts from routers or watcher
  - Ensure the initial `Handshake` was sent and the HMAC key matches (`kdapp-demo-secret` in examples).
