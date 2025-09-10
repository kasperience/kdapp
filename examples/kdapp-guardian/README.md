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
  log_level = "info"                   # log verbosity
  # state_path = "guardian_state.json"   # optional persisted state file
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

2) Run kdapp-merchant and include the guardian address/key so disputes are forwarded to the guardian:

   ```sh
   cargo run -p kdapp-merchant -- \
     --guardian-addr 127.0.0.1:9650 \
     --guardian-key <guardian_pubkey_hex> \
     create --episode-id 42 --invoice-id 1 --amount 1000
   # then open a dispute by canceling the invoice before it is paid
   cargo run -p kdapp-merchant -- cancel --episode-id 42 --invoice-id 1
   ```

   After the cancel, the guardian signs the refund. In guardian logs you should see:

   - `guardian: co-signed refund for episode 42`.

## Troubleshooting

- wrpc connect errors: verify `wrpc_url` and network (`mainnet=false` uses testnet-10).
- unknown episode warnings: a refund was escalated for an episode the guardian hasn’t observed yet; once checkpoints arrive (or another escalate occurs) the dispute will be tracked.
