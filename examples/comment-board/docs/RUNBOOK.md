# Comment Board Runbook (P2PK Bonds + Experimental Scripts)

This runbook captures how to operate the comment-board example today, what
feature flags exist, and how to perform live verification (policy injection).

## Prereqs
- Testnet‑10 KAS in each wallet. Faucet: https://faucet.kaspanet.io/
- One organizer and any number of participants. Each has a Kaspa schnorr private key (hex).
- Optional: a stable wRPC URL (`--wrpc-url wss://host:port`).

## Organizer (create or attach to room)
- New room (P2PK bonds by default):
  - `cargo run -p comment-board -- --kaspa-private-key <hex> --bonds`
  - Copy the printed `Episode ID` and share it.
- Attach to existing room:
  - `cargo run -p comment-board -- --kaspa-private-key <hex> --room-episode-id <id> --bonds`

## Participant (join room)
- Join (P2PK bonds):
  - `cargo run -p comment-board -- --kaspa-private-key <hex> --room-episode-id <id> --bonds`
- Explicit P2PK policy (optional):
  - `--bond-script-descriptor p2pk`

## Experimental features
- Framework tx context feature (optional):
  - Build with: `cargo run -p comment-board --features kdapp/tx-script-bytes -- ...`
  - Proxy provides per-output `script_bytes`, enabling descriptor verification in episodes.
- Script policy injection (for live verification):
  - Force descriptor: `--bond-script-descriptor timelock` (declared policy TimeLock)
  - If the on-chain bond output is P2PK, the episode rejects with
    `bond script policy not recognized on-chain`.
- Experimental script-bonds (non‑standard on public nodes):
  - `--script-bonds` attempts a script-locked output. Expect rejection as "non‑standard script form".

## What the episode enforces today
- On‑chain value check: requires `outputs[0].value == bond_amount` in the combined comment+bond tx.
- Descriptor check (when `script_bytes` available):
  - If `bond_script` declared, decodes on-chain descriptor from `script_bytes` and compares.
  - Rejects when declared policy (e.g., TimeLock) doesn't match on-chain (e.g., P2PK).

## Robustness improvements in this branch
- Multi‑input funding for the combined comment+bond tx (no auto-splitting required).
- Dynamic fee escalation on node feedback (raises fee if minimum required > 5000 sompi).
- Orphan backoff: retries submit briefly when node reports "orphan".
- Resilient auth handshake: timeout + one retry of challenge/response.

## Known behavior / tips
- Auto UTXO split: public nodes often reject splits due to mass; warnings are harmless.
- Windows file locks (access denied on rebuild):
  - Kill running `comment-board.exe`, or run from a different target dir:
    - PowerShell: `$env:CARGO_TARGET_DIR="target-join"; cargo run -p comment-board -- ...`
  - Or run the compiled binary directly from `target\\debug`.
- Use explicit `--wrpc-url` if your public node drops the WebSocket.

## Windows: file lock errors during rebuild
That error is Windows-specific and unrelated to join logic. It happens when Cargo tries to rebuild but the previously run binary is still locked by the OS.

- Run without rebuilding (if binary already built):
  - `.\\target\\debug\\comment-board.exe --kaspa-private-key <hex> --room-episode-id <id> --bonds`
- Build to a separate target dir so the organizer’s binary isn’t locked:
  - `PowerShell: $env:CARGO_TARGET_DIR="target-join"; cargo run -p comment-board -- --kaspa-private-key <hex> --room-episode-id <id> --bonds`
- Use release target (separate EXE path):
  - `cargo run -p comment-board --release -- --kaspa-private-key <hex> --room-episode-id <id> --bonds`
- If all else fails, restart the terminal (or Windows) to clear file handles.

### Recommended participant join (P2PK bonds)
- After applying one of the above:
  - `cargo run -p comment-board -- --kaspa-private-key <hex> --room-episode-id 2141213973 --bonds`
  - Optional (explicit): `--bond-script-descriptor p2pk`
  - Optional (stable node): `--wrpc-url wss://host:port`

### Tip
- If you’re running organizer and participant from the same repo concurrently, prefer separate target dirs:
  - Organizer: default target
  - Participant: `$env:CARGO_TARGET_DIR="target-join"`
  
This avoids Windows locking when rebuilding one while the other is running.

## Live verification examples
- Accept (P2PK):
  - `cargo run -p comment-board --features kdapp/tx-script-bytes -- --kaspa-private-key <hex> --room-episode-id <id> --bonds --bond-script-descriptor p2pk`
- Reject (policy mismatch):
  - `cargo run -p comment-board --features kdapp/tx-script-bytes -- --kaspa-private-key <hex> --room-episode-id <id> --bonds --bond-script-descriptor timelock`

## Tests (optional)
- Feature‑gated descriptor mismatch test:
  - `cargo test -p comment-board --features tx-script-bytes-test`
