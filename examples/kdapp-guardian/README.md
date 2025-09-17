# OnlyKAS Guardian

The guardian is a lightweight watchtower that observes OnlyKAS programs and helps resolve disputes.
It watches the Kaspa DAG for checkpoint anchors, tracks episode sequences, and co‑signs refunds
when a customer or merchant escalates a dispute.

## Installation

```bash
cargo build -p kdapp-guardian --bin guardian-service
```

## Configuration

The service is configured via TOML or CLI flags:

| Flag | Description |
| ---- | ----------- |
| `--listen-addr` | UDP socket for TLV messages from merchants and customers. |
| `--wrpc-url` | Kaspa wRPC endpoint used to watch the DAG for `OKCP` checkpoints. |
| `--mainnet` | Set to `true` to connect to mainnet (default is testnet‑10). |
| `--key-path` | Location or descriptor of the guardian's secp256k1 private key. Use a filesystem path or `hsm:ENV_VAR` to load material supplied by an HSM agent. Files are created on first run. **Keep this material secret.** |
| `--state-path` | Optional path used to persist dispute status and sequence counters. |
| `--http-port` | Port for health and metrics endpoints. Defaults to `listen_port + 1`. |

Example `guardian.toml`:

```toml
listen_addr = "127.0.0.1:9650"
wrpc_url = "wss://node:16110"
mainnet = false
key_path = "guardian.key"
state_path = "guardian.state"
http_port = 9651
```

The guardian writes its key file if one does not exist.  The file permissions are restricted to the
current user on Unix systems.  The `state_path` stores open disputes and sequence numbers so the
guardian can recover after restarts.

### Key management and rotation

Guardians may load signing keys from the local filesystem or from an HSM/remote signer.  Set
`key_path = "path/to/guardian.key"` for file-based storage, or `key_path = "hsm:ENV_VAR"` to have
the service read hex-encoded key material from the `ENV_VAR` environment variable (for example when
an HSM daemon exposes an export or handle).  Prefixes such as `hsm://slot-name` or `hsm:env:VAR` map
to the same environment variable mechanism; if no name is provided (`hsm:`) the default variable
`GUARDIAN_HSM_KEY` is used.

- **Secure generation.** Provision production keys outside the guardian host and copy them into
  place with `chmod 600` permissions. For a file backend, `openssl rand -hex 32 > guardian.key` is a
  simple starting point; for HSM deployments, use the vendor tooling to create a signing key and set
  `ENV_VAR` to the exported handle or wrapped secret.
- **Rotation.** Generate the replacement key, update `key_path` (or the referenced environment
  variable), restart the guardian, and re-run handshakes with merchants/customers so they learn the
  new guardian pubkey. Keep the prior key available until all sessions acknowledge the rotation.
- **Hygiene.** Remove stale key files once rotation is complete and clear environment variables after
  restarts. Pair the guardian with a secret manager or HSM to avoid storing raw keys on disk long
  term.

Generate the config file manually or by copying the example above.

## Running

Start the service with a config file:

```bash
guardian-service --config guardian.toml
```

Alternatively, configuration options can be supplied as flags:

```bash
guardian-service --listen-addr 0.0.0.0:9650 --wrpc-url wss://node:16110 --http-port 9651 --key-path guardian.key
```

The UDP listener binds to `listen_addr`, the wRPC client connects to `wrpc_url`, and a small HTTP
server exposes metrics on `http_port`.

Check the health and metrics endpoints:

```bash
curl http://127.0.0.1:9651/healthz
curl http://127.0.0.1:9651/metrics
```

## Guardian workflow

1. **Handshake** – merchants and customers establish a shared HMAC channel with the guardian.
2. **Escalate** – when a payment is disputed, an escalation TLV is sent to the guardian with a
   refund transaction.
3. **Confirm** – once the refund is co‑signed by the guardian, the merchant acknowledges the
   resolution.
4. **Refund signing** – the guardian signs the refund transaction and returns the signature in the
   TLV response. The watcher verifies this signature before broadcasting the refund.

### Dispute example

1. Merchant sends `Escalate` TLV with a refund transaction.
2. Guardian validates and replies with `Confirm`.
3. Merchant acknowledges resolution; guardian state persists to `state_path`.

Guardians automatically scan the Kaspa DAG for compact `OKCP` checkpoints.  If an episode's
sequence number skips or replays, the guardian opens a dispute and awaits an escalation message.

## Next steps

See the [merchant README](../kdapp-merchant/README.md) for integrating a merchant with a guardian and
processing customer payments.  The [top‑level README](../../README.md) describes the repository
layout and provides broader context for OnlyKAS.
