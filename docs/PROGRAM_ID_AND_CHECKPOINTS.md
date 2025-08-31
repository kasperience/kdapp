# kdapp RFC: Program-ID & Checkpoints (M1 → M2)

Purpose. Make episode contracts immutable and publicly verifiable on Kaspa without an L1 VM: anchor the code once, carry state commitments off‑chain, checkpoint occasionally on‑chain, and use simple L1 spend rules for settlement.

## Invariants (bold rules)
- Immutable code identity: Every episode declares a fixed `PROGRAM_ID = BLAKE2b_256(canonical_source_bundle)`.
- Deterministic state: The state serializes canonically; equal states hash equal across nodes.
- Ordered transitions: Off‑chain messages increase `seq` strictly: `seq_n = seq_{n-1} + 1`.
- Enforceable money paths: Settlement uses simple L1 scripts (timeouts, penalties, commit‑reveal) so funds can be recovered fairly.

## Program-ID (code anchoring)

### Canonical bundle (cross‑platform)
Produce a reproducible archive of the episode’s source (no timestamps, stable order, LF line endings):

```sh
# From the episode crate root
# Ensure LF endings (optional)
# git ls-files -z | xargs -0 dos2unix --allow-chown

# Create canonical bundle
git archive --format=tar --prefix=ep/ HEAD | gzip -n -9 > bundle.tar.gz

# PROGRAM_ID is BLAKE2b_256 of bundle bytes
```

Define:

```
PROGRAM_ID = BLAKE2b_256(canonical_bundle_bytes)
```

Record the exact Git commit and the command you used in `docs/REPRODUCIBLE_BUILD.md`.

### On‑chain anchoring (pick ONE)

A) Pay‑to‑Contract (data‑minimal; preferred)

Let `P` be a base secp256k1 pubkey. Derive:

```
Q = P + H(tag="onlyKAS:program", P || PROGRAM_ID) · G
```

Use `Q` in a standard spend path (e.g., single‑sig or 2‑of‑2). Publish `(P, PROGRAM_ID, Q)` in docs so anyone can verify.

B) Explicit commit in script (simpler to inspect)

Include `PROGRAM_ID` in the script, then `OP_DROP`, then a standard spend path:

```
PUSH32 <PROGRAM_ID> OP_DROP <standard single‑sig or 2‑of‑2 template>
```

Record `genesis_txid:vout`, `method (A|B)`, and derivation details in `docs/PROGRAM_ID.md`.

## Deterministic state & root

- Serialization: Use Borsh (or Serde+CBOR) with explicit field order.
- Use `BTreeMap` (or sorted iterators) for all map‑like fields.
- Define:

```
STATE_ROOT = BLAKE2b_256( serialize_canonical(state) )
```

## Off‑chain envelope (onlyKAS TLV v1)

Each message carries the minimum to verify transitions:

```
version:     u8     (=1)
type:        u8     (0=New, 1=Cmd, 2=Ack, 3=Close, 4=AckClose, 5=Checkpoint)
episode_id:  u64
seq:         u64        # must be monotone, start at 0
state_root:  [32]
payload_len: u16
payload:     [u8]       # serialized EpisodeMessage, or empty for Checkpoint
```

`PROGRAM_ID` is not repeated every message: bind it at genesis and cache from the episode’s New.

## On‑chain checkpoints (public audit trail)

A small, fixed record any watcher can index:

```
magic:      [4]   = "OKCP"
version:    u8    = 1
episode_id: u64
seq:        u64
state_root: [32]
```

Embed in a tiny commitment output (pay‑to‑contract or explicit push+DROP). Target size: ≤ 80–100 bytes total when included via kdapp payload output.

Publish at milestones (e.g., after ExecuteDraw) or every N minutes.

## Watchtower responsibilities

- Verify ordering: reject or flag non‑monotone `seq`.
- Recompute transitions: using the anchored code at `PROGRAM_ID`, check `f(state, cmd) → new_state_root`.
- Mirror CSV/CLTV windows: for settlement safety.
- Alert/punish: on stale channel closes (penalty path) or missing claims/timeouts.

## Engine / message tweaks

- NewEpisode should carry `program_id: [u8;32]` (cache per `episode_id`).
- Compute and attach `state_root` after every accepted command.
- For checkpoint CLI, emit `{episode_id, seq, state_root}` with a dedicated kdapp `PREFIX`.

## Threat model (short)

- Old‑state publication: stopped by penalty + CSV and watchtowers.
- State equivocation: prevented by strict `seq` and public checkpoints.
- Non‑determinism: avoided by canonical serialization and sorted maps.
- Oracle bias (if used): minimized with commit‑reveal and fixed draw rules; document assumptions.

## Status & Roadmap

- M1: Anchor PROGRAM_ID; L1 MVP with deterministic STATE_ROOT; optional single checkpoint.
- M2: Full onlyKAS transport; towers verify transitions; periodic checkpoints.
- Later: dispute gadgets (optional), richer per‑episode spend hints.

