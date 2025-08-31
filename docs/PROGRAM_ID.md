# Program ID — kas-draw

- program_id: $id
- crate: xamples/kas-draw
- computed_at: 2025-08-31T15:33:52
- git_commit: a0f1c1c9899257083b742472d89ca9ded3f557a7
- method: canonical tar via git archive --format=tar --prefix=ep/ HEAD (Windows fallback; gzip not found)
  - Note: If gzip is available, preferred bundle is tar.gz with gzip -n -9 for deterministic bytes. The value will change if you switch bundle method — pick one and stick to it.

## Recompute

- Windows (current repo state):
  - cargo run -p kas-draw --bin program_id
- POSIX (with gzip):
  - git archive --format=tar --prefix=ep/ HEAD | gzip -n -9 | b2sum | awk '{print }' | cut -c1-64

## Anchor (to do)

Pick one method described in docs/PROGRAM_ID_AND_CHECKPOINTS.md and record details here once anchored:

- method: A) Pay-to-Contract | B) Commit-in-Script | C) Data-only
- genesis_txid:vout: <fill>
- base_pubkey P (for A): <fill>
- derived_pubkey Q (for A): <fill>

