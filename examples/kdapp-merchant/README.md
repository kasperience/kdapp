# kdapp-merchant

This example demonstrates a simple merchant application built on kdapp. It includes an optional checkpoint watcher that anchors state hashes to the Kaspa network.

## Watcher Configuration

The watcher fee and congestion behaviour can be tuned using two parameters:

- `max_fee` – maximum fee (in sompis) for an anchoring transaction. The watcher defers if the estimated fee exceeds this limit. **Default:** no limit. **Recommended range:** 5,000 – 100,000.
- `congestion_threshold` – mempool congestion ratio above which anchoring is deferred. **Default:** 0.7. **Recommended range:** 0.5 – 0.9.

### Via CLI

Provide these options when starting either the `serve` or `watcher` subcommands:

```bash
kdapp-merchant serve --max-fee 50000 --congestion-threshold 0.8
kdapp-merchant watcher --max-fee 50000 --congestion-threshold 0.8
```

### Via HTTP

When running the `serve` subcommand, the watcher settings can be updated at runtime:

```http
POST /watcher-config
Content-Type: application/json
x-api-key: <API_KEY>

{
  "max_fee": 50000,
  "congestion_threshold": 0.8
}
```

The provided values apply to the currently running watcher process.

## Mempool Metrics

The watcher tracks the most recent fee estimate and a simple mempool congestion ratio. These metrics help decide when to anchor
checkpoints.

### Via CLI

```bash
kdapp-merchant watcher --show-metrics
```

Outputs the current `base_fee` (sompis required for a small transaction) and `congestion` ratio.

### Via HTTP

```http
GET /mempool-metrics

{
  "base_fee": 5000,
  "congestion": 0.42
}
```

- `base_fee` – conservative fee in sompis for anchoring transactions.
- `congestion` – mempool size ratio (higher values indicate a busier mempool).
