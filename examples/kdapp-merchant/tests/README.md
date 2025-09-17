# kdapp-merchant tests

This directory contains integration tests for the merchant example, including end-to-end flows with customer and guardian components.

## Running

From the repository root run:

```bash
cargo test -p kdapp-merchant
```

To run just the new integration tests:

```bash
cargo test -p kdapp-merchant invoice_flow
```

## Reorg regression harness

End-to-end reorg handling lives in the `examples/tests` crate. The `merchant_reorg`
test spins up a real `Engine<ReceiptEpisode, _>` instance, injects
`EngineMsg::BlkAccepted` events for invoice creation/payment, then rewinds them
with `EngineMsg::BlkReverted` to emulate a Kaspa block rollback. It validates two
behaviours:

* Invoice confirmation records drop back to `None` after the revert and are
  repopulated with the lower confirmation depth once the payment is
  re-accepted.
* The "invoice_paid" webhook pathway re-fires (captured by the test handler) on
  the re-accept path, ensuring external listeners get a second notification.

Run it with:

```bash
cargo test -p examples-tests merchant_reorg
```
