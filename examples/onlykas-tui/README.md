# onlykas TUI Demo

This terminal UI orchestrates a merchant, watcher, and guardian to showcase how onlyKAS components interact.

## Prerequisites
- Rust toolchain
- Running merchant and guardian services

## Build
```bash
cargo build -p onlykas-tui
```

## Configure Webhook
The TUI starts an HTTP listener on `127.0.0.1` and prints the chosen port on launch.
Start the merchant with:
```
--webhook-url http://127.0.0.1:<PORT>/hook --webhook-secret <secret>
```
Use the same secret when running the TUI.

## Mock Mode Demo
1. **Merchant**
   ```bash
   cargo run -p kdapp-merchant -- serve \
     --bind 127.0.0.1:3000 \
     --webhook-url http://127.0.0.1:<PORT>/hook \
     --webhook-secret deadbeef
   ```
2. **Guardian**
   ```bash
   cargo run -p kdapp-guardian --bin guardian-service -- \
     --listen-addr 127.0.0.1:9650
   ```
3. **TUI**
   ```bash
   cargo run -p onlykas-tui -- \
     --merchant-url http://127.0.0.1:3000 \
     --guardian-url http://127.0.0.1:9650 \
     --webhook-secret deadbeef \
     --mock-l1
   ```

## Real Mode
Run the services as above but omit `--mock-l1` from the TUI.
Pay invoices using a real Kaspa wallet and wait for the transaction to confirm on L1.

## Key Bindings & Panels
### Keys
| Key | Action |
| --- | --- |
| `n` | Create a new invoice |
| `p` | Simulate payment (mock mode) |
| `a` | Acknowledge the selected invoice |
| `d` | Open a dispute for the selected invoice |
| `w` | Configure watcher fee policy |
| `r` | Refresh data from services |
| `q` | Quit |
Arrow keys navigate lists and `Tab` toggles between invoices and subscriptions.

### Panels
- **Actions** – shortcut reference.
- **Invoices/Subscriptions** – list items; use `Tab` to switch modes.
- **Watcher** – mempool and fee statistics.
- **Guardian** – dispute and refund metrics.
- **Webhooks** – recent events delivered via the webhook endpoint.

## Troubleshooting
- Terminal must support Unicode block characters; without it the UI may misrender.
- The ASCII logo at the top should display "onlyKAS". Adjust font/encoding if it does not.
