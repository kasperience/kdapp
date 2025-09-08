# kdapp-guardian

This example shows a minimal guardian service that observes checkpoint
anchors and assists merchants and customers during disputes.

## Running the guardian

The `service::run` helper starts a UDP listener for guardian messages
and subscribes to Kaspa wRPC notifications for checkpoint anchors:

```rust
fn main() {
    let _state = kdapp_guardian::service::run("127.0.0.1:9650", None);
    std::thread::park();
}
```

The returned `GuardianState` is shared and updated as anchors are
observed on-chain.

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
