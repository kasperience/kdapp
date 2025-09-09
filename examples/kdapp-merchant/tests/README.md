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
