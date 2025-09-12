# OnlyKAS Merchant

The merchant example showcases a pay‑per‑invoice workflow with optional subscriptions. It bundles a
router for TLV messages, a watcher that anchors checkpoints, a scheduler for recurring charges, and
an HTTP server for integrating external systems.

## Quickstart

```bash
kdapp-merchant serve --bind 127.0.0.1:3000 --episode-id 1 \
  --merchant-private-key <hex> --api-key secret \
  --webhook-url http://127.0.0.1:4000/hook --webhook-secret deadbeef
```

Create, pay, and acknowledge an invoice:

```bash
curl -X POST http://127.0.0.1:3000/invoice -H 'x-api-key: secret' \
  -d '{"invoice_id":1,"amount":1000}'
curl -X POST http://127.0.0.1:3000/pay -H 'x-api-key: secret' \
  -d '{"invoice_id":1,"payer_public_key":"<hex>"}'
curl -X POST http://127.0.0.1:3000/ack -H 'x-api-key: secret' \
  -d '{"invoice_id":1}'
```

## Installation

```bash
cargo build -p kdapp-merchant
```

## Demo mode

Run a full in‑process demo that creates, pays, and acknowledges an invoice:

```bash
kdapp-merchant demo
```

The command spins up the engine, router, and scheduler, then prints a demo customer's private key
for experimentation.

## Watcher configuration

The watcher anchors compact `OKCP` checkpoints on Kaspa and exposes mempool metrics.  Fee policies
can be tuned via CLI flags or at runtime through the HTTP API.

### CLI flags

```bash
# static fee policy
kdapp-merchant watcher --fee-policy static --static-fee 5000 --http-port 9591

# congestion-aware policy
kdapp-merchant watcher \
  --fee-policy congestion \
  --min-fee 5000 --max-fee 100000 \
  --defer-threshold 0.7 --multiplier 1.0 \
  --http-port 9591
```

- `--max-fee` – cap fee for anchoring transactions.
- `--defer-threshold` – skip anchoring when congestion ratio exceeds this value.
- `--http-port` – expose mempool metrics on `/mempool`.

Query current metrics:

```bash
curl http://127.0.0.1:9591/mempool
```

The watcher selects a fee based on `static` or `congestion` policy. Runtime overrides can be sent to
`POST /watcher-config`.

## Guardian integration

Merchants may delegate dispute resolution to guardians.  Supply guardian UDP addresses and public
keys when invoking commands or running the demo:

```bash
kdapp-merchant --guardian-addr 127.0.0.1:9650 \
  --guardian-key <guardian_pubkey_hex> demo
```

During a dispute, the merchant sends an escalation TLV to the guardian.  The guardian co‑signs the
refund and the watcher verifies the signature before broadcasting it.

## HTTP API quickstart

Start the HTTP server:

```bash
kdapp-merchant serve \
  --bind 127.0.0.1:3000 \
  --episode-id 1 \
  --api-key secret \
  --merchant-private-key <hex>
```

Endpoints use `x-api-key` for authentication:

| Endpoint | Description |
| -------- | ----------- |
| `POST /invoice` | Create an invoice `{invoice_id, amount, memo?, guardian_public_keys?}` |
| `POST /pay` | Mark an invoice paid `{invoice_id, payer_public_key}` |
| `POST /ack` | Acknowledge payment `{invoice_id}` |
| `POST /cancel` | Cancel an open invoice `{invoice_id}` |
| `POST /subscribe` | Create a subscription `{subscription_id, customer_public_key, amount, interval}` |
| `POST /subscriptions/:id/charge` | Trigger a subscription charge immediately |
| `POST /subscriptions/:id/disputes` | Escalate a subscription dispute |
| `GET /invoices` | List invoices |
| `GET /subscriptions` | List subscriptions |
| `POST /watcher-config` | Override watcher `max_fee` or `congestion_threshold` |
| `GET /mempool-metrics` | Fetch mempool metrics snapshot |

Example usage:

```bash
# create an invoice
curl -X POST http://127.0.0.1:3000/invoice \ 
  -H 'x-api-key: secret' \ 
  -d '{"invoice_id":1,"amount":1000}'

# pay it
curl -X POST http://127.0.0.1:3000/pay \ 
  -H 'x-api-key: secret' \ 
  -d '{"invoice_id":1,"payer_public_key":"<hex>"}'
```

## Scheduler and subscriptions

The scheduler thread scans stored subscriptions every ten seconds.  When a charge fails, it retries
with exponential backoff starting at five seconds and capping at five minutes.

### Subscription flow

1. `POST /subscribe` to create a subscription.
2. Scheduler issues `charge` TLV on schedule.
3. Merchant `POST /ack` to acknowledge and emits an invoice.

## Webhooks

Supply `--webhook-url` and `--webhook-secret` when running `serve` to receive HMAC‑signed JSON
callbacks:

- `invoice_created`
- `invoice_paid`
- `invoice_acked`
- `invoice_cancelled`

Webhook events fire in order: `invoice_created` → `invoice_paid` → `invoice_acked`.

## Pay-per-invoice flow

1. Merchant creates an invoice (`POST /invoice`).
2. Customer pays it (`POST /pay`).
3. Merchant acknowledges (`POST /ack`).
4. Watcher anchors the checkpoint to Kaspa.

## Example flow

1. Run the watcher to anchor checkpoints and expose mempool metrics.
2. Start the HTTP server with an API key and merchant private key.
3. A customer creates and pays an invoice via HTTP or CLI.
4. The merchant acknowledges the payment or escalates a dispute.
5. If disputed, the guardian co‑signs the refund and the watcher confirms it.

## References

- [Guardian README](../kdapp-guardian/README.md)
- [Top‑level README](../../README.md)
