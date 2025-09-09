# kdapp-guardian

Minimal guardian service that watches checkpoint anchors and helps resolve disputes by co-signing refunds.

## Quickstart

- Create a config file (TOML):

  ```toml
  # config.toml
  listen_addr = "127.0.0.1:9650"      # UDP for guardian TLV
  wrpc_url = "wss://node:16110"       # Kaspa wRPC endpoint (testnet-10 or mainnet)
  mainnet = false                      # false = testnet-10
  key_path = "guardian.key"            # will be created if missing
  state_path = "guardian_state.json"   # optional, persists disputes + signatures
  ```

- Run the service:

  ```sh
  cargo run -p kdapp-guardian --bin guardian-service -- --config config.toml
  ```

  On startup it prints the guardian public key (hex). Use this when configuring merchants/customers.

## What it does

- UDP TLV listener: accepts `Handshake`, `Escalate`, and `Confirm` messages signed with a shared HMAC key.
- Anchor watcher (wRPC): connects to Kaspa and scans accepted virtual blocks for compact OKCP checkpoint records (prefix `KMCP`).
  - Tracks per-episode sequences and marks an episode as disputed if a replay or gap is seen.
- Refund co-sign: upon a valid `Escalate` that includes a refund transaction, the guardian signs the refund with its private key.
- Watcher notification: after co-signing, it notifies the checkpoint watcher (UDP `127.0.0.1:9590`) with a `Refund` TLV so the watcher can verify and log the refund.
- Optional persistence: when `state_path` is set, maintains disputes, last sequences and signed refunds across restarts.

## HTTP endpoints

- Health: `GET http://<host>:<listen_port+1>/healthz` → `ok`
- Metrics: `GET http://<host>:<listen_port+1>/metrics`

Example metrics JSON:

```json
{
  "valid": 12,
  "invalid": 1,
  "disputes": 2,
  "observed_payments": 3,
  "guardian_refunds": 1
}
```

Fields:
- valid/invalid: count of accepted vs rejected guardian TLV messages (HMAC/ordering).
- disputes: number of open disputes (episodes with sequence discrepancies or escalations).
- observed_payments: `Escalate` messages received that referenced an invoice/payment.
- guardian_refunds: refunds the guardian has co-signed (size of the signature store).

## End-to-end on testnet

1) Start the guardian (see Quickstart). Note the printed guardian public key (hex).

2) Start the kdapp-merchant watcher on the same machine so it listens on UDP `127.0.0.1:9590`:

   ```sh
   cargo run -p kdapp-merchant -- watcher --kaspa-private-key <hex> --wrpc-url wss://node:16110 --http-port 9591
   ```

   The watcher anchors OKCP checkpoints on-chain and exposes `GET http://127.0.0.1:9591/mempool`.

3) Run kdapp-merchant and include the guardian address/key so disputes are forwarded to the guardian:

   ```sh
   cargo run -p kdapp-merchant -- \
     --guardian-addr 127.0.0.1:9650 \
     --guardian-key <guardian_pubkey_hex> \
     create --episode-id 42 --invoice-id 1 --amount 1000
   # then open a dispute by canceling the invoice before it is paid
   cargo run -p kdapp-merchant -- cancel --episode-id 42 --invoice-id 1
   ```

   After the cancel, the guardian signs the refund and notifies the watcher. In watcher logs you should see:

   - `checkpoint received: ep=42 seq=...` (periodic)
   - `refund verified for ep=42 seq=0`

   In guardian logs: `guardian: co-signed refund and notified watcher for episode 42`.

## Troubleshooting

- wrpc connect errors: verify `wrpc_url` and network (`mainnet=false` uses testnet-10).
- unknown episode warnings: a refund was escalated for an episode the guardian hasn’t observed yet; once checkpoints arrive (or another escalate occurs) the dispute will be tracked.
- ack timeout when notifying watcher: the watcher does not ack `Refund`; this is expected. The guardian still sends the notification once.

