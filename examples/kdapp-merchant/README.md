# kdapp-merchant

This example demonstrates a simple merchant application built on kdapp. It includes an optional checkpoint watcher that anchors state hashes to the Kaspa network.

## Watcher Configuration

The watcher fee and congestion behaviour can be tuned using two parameters:

- `max_fee` – maximum fee (in sompis) for an anchoring transaction. The watcher defers if the estimated fee exceeds this limit. **Default:** no limit. **Recommended range:** 5,000 – 100,000.
- `congestion_threshold` – mempool congestion ratio above which anchoring is deferred. **Default:** 0.7. **Recommended range:** 0.5 – 0.9.

### Via CLI

Provide these options when starting either the `serve` or `watch` subcommands:

```bash
kdapp-merchant serve --max-fee 50000 --congestion-threshold 0.8
kdapp-merchant watch --max-fee 50000 --congestion-threshold 0.8
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
