# onlykas TUI Demo

This read-only terminal UI orchestrates a merchant, watcher, and guardian to showcase how onlykas components interact.

## Run

```bash
cargo run -p onlykas-tui -- <flags>
```

## CLI Flags

- `--merchant-url`: URL for the merchant service
- `--guardian-url`: URL for the guardian service
- `--webhook-secret`: shared secret used to verify webhooks
- `--mock-l1`: enable a mocked L1 instead of the real network
