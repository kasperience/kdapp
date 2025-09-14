# onlykas TUI Demo

This terminal UI orchestrates a merchant, watcher, and guardian to showcase how onlyKAS components interact.

## Prerequisites
- Rust toolchain
- Running merchant and guardian services

## Build
```bash
cargo build -p onlykas-tui
```

## Configure Webhook & API Key
- The TUI starts an HTTP listener on `127.0.0.1` and prints the chosen port on launch (or set it via `--webhook-port`).
- The merchant server requires an API key and validates the `x-api-key` header on every request.

Steps:
- Pick an API key string (any value; hex recommended), e.g. `deadbeefdeadbeefdeadbeefdeadbeef`.
- Start merchant with both `--webhook-*` flags and `--api-key <key>`.
- Start TUI with the same `--api-key <key>` and `--webhook-secret`.
- If you omit or mistype the key, the TUI will prompt you to enter it on the first 401 response.

Example merchant flags:
```
--webhook-url http://127.0.0.1:<PORT>/hook \
--webhook-secret <secret-hex> \
--api-key <api-key>
```
Example TUI flags:
```
--webhook-secret <secret-hex> --api-key <api-key>
```

## Mock Mode Demo
1. **Merchant**
   ```bash
   cargo run -p kdapp-merchant -- serve \
     --bind 127.0.0.1:3000 \
     --webhook-url http://127.0.0.1:<PORT>/hook \
     --webhook-secret deadbeef \
     --api-key deadbeefdeadbeefdeadbeefdeadbeef \
     --episode-id 42
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
     --api-key deadbeefdeadbeefdeadbeefdeadbeef \
     --mock-l1
   ```

## Real Mode
Run the services as above but omit `--mock-l1` from the TUI.
Pay invoices using a real Kaspa wallet and wait for the transaction to confirm on L1.

## Autopilot (One Command)
For quick demos without juggling terminals, use the bundled scripts to launch the merchant server, watcher, guardian, and TUI together. A Kaspa wRPC URL is optional — if omitted, node auto‑resolution is used.

Windows (PowerShell):
```
cd examples/onlykas-tui
./autopilot.ps1 [-WrpcUrl wss://node:port] [-Mainnet] [-EpisodeId 42] [-MerchantPort 3000] [-WebhookPort 9655]
```

macOS/Linux (Bash):
```
cd examples/onlykas-tui
MAINNET=0 ./autopilot.sh               # auto-resolve node
# or specify explicitly:
WRPC_URL=wss://node:port MAINNET=0 ./autopilot.sh
# stop all onlykas processes launched by autopilot:
./autopilot.sh --stop
```

Notes:
- Scripts generate a demo API key, webhook secret, and private keys; by default they set `MERCHANT_DB_PATH` to a WSL/Linux‑friendly location under `$XDG_DATA_HOME/onlykas/merchant-live.db` or `~/.local/share/onlykas/merchant-live.db`.
- Watcher starts automatically and serves metrics; TUI is launched with `--watcher-url` so the Watcher panel is populated.
- Guardian starts automatically and is wired via `--guardian-url` for the Guardian panel.
- Enable logs: set `DEBUG=1` for Bash or pass `-Debug` to the PowerShell script. Logs tail in the console (PS) or go to `*.log`/`*.out` files.
- Non-storage kdapp-merchant subcommands (e.g., `addr`, `kaspa-addr`, `balance`, `router-udp`, `router-tcp`, `proxy`, `watcher`, `onchain-*`) do not open the sled DB and can run while the server holds the DB lock. For server-style commands (`serve`, `serve-proxy`, `demo`) use a unique `MERCHANT_DB_PATH` per process to avoid file lock contention.

### .env Configuration (Persist Secrets)
- A template lives at `examples/onlykas-tui/.env.example`. Copy it to `.env` and set ports if needed (`MERCHANT_PORT`, `WEBHOOK_PORT`, `WATCHER_PORT`, `GUARDIAN_PORT`).
- Optional: `WRPC_URL` (leave blank for resolver pool), `MAINNET=1` for mainnet.
- The scripts read `.env`, generate any missing secrets (`API_KEY`, `WEBHOOK_SECRET`, `MERCHANT_SK`, `KASPA_SK`), and append them to `.env` for reuse. `examples/onlykas-tui/.env` is gitignored by default.

## Flags
- `--merchant-url` URL of merchant server (required)
- `--guardian-url` URL of guardian service (optional)
- `--watcher-url` URL of watcher HTTP (optional; if set, TUI pulls mempool metrics from `<watcher-url>/mempool`)
- `--webhook-secret` HMAC secret (hex) for verifying webhook payloads (required)
- `--api-key` API key string; must match merchant `--api-key` (required)
- `--webhook-port` Bind port for the local TUI webhook (optional; default random)
- `--mock-l1` Enable simulated L1 for quick demos

## Key Bindings & Panels
### Keys
| Key | Action |
| --- | --- |
| `Left` / `Right` | Move focus between panels |
| `Up` / `Down` | Scroll within the focused list |
| `Tab` | Switch between invoice and subscription lists |
| `n` | Create a new invoice |
| `p` | Simulate payment (mock mode) |
| `a` | Acknowledge the selected invoice |
| `d` | Open a dispute for the selected invoice |
| `w` | Configure watcher fee policy |
| `r` | Refresh data from services |
| `q` or `Ctrl+C` | Quit |

Use `Left`/`Right` to change panels and `Up`/`Down` to navigate items within the active panel.

### Panels
- **Actions** – shortcut reference.
- **Invoices/Subscriptions** – list items; use `Tab` to switch modes.
- **Watcher** – mempool and fee statistics.
- **Guardian** – dispute and refund metrics.
- **Webhooks** – recent events delivered via the webhook endpoint.

## Troubleshooting
- Terminal must support Unicode block characters; without it the UI may misrender.
- The logo uses Unicode blocks. If it looks misaligned, try a monospace font (Consolas, Cascadia Mono). Or set `ONLYKAS_TUI_ASCII=1` to use a simple ASCII logo.
- Color: The "KAS" part uses teal (RGB 0,128,128). Some terminals may approximate.
- Preflight test: run `examples/scripts/smoke_onlykas.sh` to validate merchant+watcher+webhook locally before launching the TUI. It creates an invoice, simulates pay/ack, and checks webhook delivery.
- Watcher panel shows `null` if the watcher is not running or TUI is not pointed to it. Either run `kdapp-merchant watcher --http-port <P>` and start TUI with `--watcher-url http://127.0.0.1:<P>`, or expose metrics from the merchant process itself.
- Rust logs: set `RUST_LOG=info,kdapp=info,kdapp_merchant=info` on servers. Use the autopilot `DEBUG`/`-Debug` options to stream logs.
 - If mempool metrics show unavailable, ensure your wRPC URL is reachable and consider running the watcher process for anchoring checkpoints.
- Sled DB lock: If you see an error like "could not acquire lock on merchant-live.db", another process is using the same DB directory. Either stop the other process or set `MERCHANT_DB_PATH` to a unique path for each server process. Non-storage subcommands skip DB initialization and are not affected.
