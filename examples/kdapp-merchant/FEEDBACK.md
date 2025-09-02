onlyKAS Merchant — Review Feedback (M0 → M1)

Questions for Feedback & Suggestions

- Transport vs. On‑Chain First:
  - Start with the TLV‑based local router to harden the off‑chain path and establish protocol stability, then layer in on‑chain transport once the payload format and flow are firm.

- Payment Validation Depth:
  - M0: coarse check (any tx_output ≥ invoice amount) is acceptable.
  - M1: consider verifying script bytes or leveraging policy checks when the proxy supplies script data.

- Program ID & Checkpoints:
  - Near‑term: use a stable hash/tag across examples for uniform verification.
  - Long‑term: define the PROGRAM_ID format early so checkpoints and labels can reference it consistently, easing cross‑example integration.

- CLI Ergonomics:
  - Improve beyond a single `--demo` path: add subcommands (create, pay, ack, cancel, list) to clarify flows and enable targeted testing.

Future Work (Tracking)

- On‑Chain Wiring:
  - Assign unique `PrefixType`/10‑bit `PatternType` and connect via `kdapp::proxy::run_listener`.

- Episode Hardening:
  - Enrich invoice metadata, verify payer identity, and integrate script‑policy checks.

- Checkpointing:
  - Align with OKCP/KDCK‑style periodic anchors; verify with `PROGRAM_ID_AND_CHECKPOINTS.md`.

- Off‑Chain Router:
  - Implement TLV transport over UDP/TCP/WebSocket with sequence/replay protection (mirror `examples/kas-draw` patterns).

