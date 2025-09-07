# kdapp-customer

Command line demo for interacting with the merchant receipt episode.

## Subcommands

- `list` – fetch invoices from a running merchant HTTP server.
- `pay` – send a TLV `MarkPaid` command.
- `ack` – send a TLV `AckReceipt` command.

## Required arguments

`pay` and `ack` require:

- `--episode-id <id>` – numeric episode identifier.
- `--invoice-id <id>` – invoice to act on.
- A signing key:
  - `pay` uses `--payer-private-key <hex>`.
  - `ack` uses `--merchant-private-key <hex>`.

The `list` command uses HTTP and does not require these fields.

## Example

```sh
cargo run -p kdapp-customer -- pay --episode-id 42 --invoice-id 1 --payer-private-key <hex>
```
