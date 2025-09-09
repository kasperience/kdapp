# kdapp-guardian

This example shows a minimal guardian service that observes checkpoint
anchors and assists merchants and customers during disputes.

## Running the guardian

The `guardian-service` binary reads a small TOML configuration file and
starts a UDP listener for guardian messages while polling the Kaspa
virtual chain for checkpoint anchors:

```bash
guardian-service --config config.toml
```

An example config:

```toml
listen_addr = "127.0.0.1:9650"
wrpc_url = "wss://node:16110"
mainnet = false
key_path = "guardian.key"
```

Under the hood the service uses `get_block_dag_info` +
`get_virtual_chain_from_block` to follow accepted blocks and scans their
merged blocks for compact OKCP records (program prefix `KMCP`). The
returned `GuardianState` is shared and updated as anchors are observed
on‑chain.

## Using with merchant and customer

Both the merchant and customer binaries accept the guardian's UDP
address and public key:

```
# merchant
cargo run -p kdapp-merchant -- --guardian-addr 127.0.0.1:9650 --guardian-public-key <hex>

# customer
cargo run -p kdapp-customer -- --guardian-addr 127.0.0.1:9650 --guardian-public-key <hex> ...
```

During normal operation each side performs a `Handshake` with the
guardian and periodically sends `Confirm` messages referencing the
latest checkpoint sequence. If a customer detects a problem it may
send an `Escalate` message which causes the guardian to verify the
latest checkpoint and co‑sign a refund or release transaction.

## Dispute flow example

1. The merchant observes a conflicting payment and forwards a dispute:

   ```rust
   guardian::send_escalate("127.0.0.1:9650", 1, "payment dispute".into(), refund_tx.clone(), guardian::DEMO_HMAC_KEY);
   ```

2. The guardian's watcher detects the next `OKCP` anchor and notices a
   gap in the sequence, recording an open dispute in `GuardianState`.

3. When the `Escalate` message arrives the guardian verifies the
   checkpoint and signs the provided refund transaction using the
   demo keypair. The merchant can then broadcast the refund once the
   signature is verified.
