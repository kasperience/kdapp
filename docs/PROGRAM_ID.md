# Program ID — kas-draw (plain English)

What is this?
- Program‑ID is a fingerprint of the code that runs this app. Same code → same ID. If the code changes, the ID changes.
- We will anchor this ID on‑chain once so everyone can prove “we’re running this exact code”.

Current value
- program_id: `03156426f8d0c302d84876c53c1743469f41e9709c9a5bbec3ef87673a525aff`
- crate: `examples/kas-draw`
- computed_at: 2025-08-31T15:33:52
- git_commit (at compute time): a0f1c1c9899257083b742472d89ca9ded3f557a7
- method: canonical tar via `git archive --format=tar --prefix=ep/ HEAD` (Windows fallback; gzip not found)
  - Note: If `gzip` is available, the preferred bundle is tar.gz with `gzip -n -9` for deterministic bytes. Pick one method and stick to it for consistency.

Recompute
- Windows (this repo):
  - `cargo run -p kas-draw --bin program_id`
- POSIX (with gzip):
  - `git archive --format=tar --prefix=ep/ HEAD | gzip -n -9 | b2sum | awk '{print $1}' | cut -c1-64`

Anchor (to do)
Pick one method from `docs/PROGRAM_ID_AND_CHECKPOINTS.md` and record the details here once anchored:
- method: A) Pay‑to‑Contract | B) Commit‑in‑Script | C) Data‑only
- genesis_txid:vout: <fill>
- base_pubkey P (for A): <fill>
- derived_pubkey Q (for A): <fill>

Why it matters
- Anyone can rebuild the code → check the same Program‑ID → trust that the checkpoints/state roots really come from this code.

