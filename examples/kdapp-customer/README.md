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

## Testing with kdapp-merchant

1) Start the merchant UDP router (requires handshake + signed TLV):

```
cargo run -p kdapp-merchant -- router-udp --bind 127.0.0.1:9530
```

2) Optional: Start the watcher to anchor checkpoints on-chain:

```
cargo run -p kdapp-merchant -- watch --bind 127.0.0.1:9590 --kaspa-private-key <hex> [--wrpc-url wss://host:port] [--mainnet]
```

3) Perform actions with kdapp-customer (the client performs a handshake automatically):

```
cargo run -p kdapp-customer -- pay --episode-id 42 --invoice-id 1001 --payer-private-key <hex>
cargo run -p kdapp-customer -- ack --episode-id 42 --invoice-id 1001 --merchant-private-key <hex>
```

Expected:
- The router logs a `Handshake` ack and then acknowledges signed messages with `Ack`/`AckClose`.
- The merchant emits periodic/signed checkpoints; the watcher (if running) submits OKCP anchors on-chain.
