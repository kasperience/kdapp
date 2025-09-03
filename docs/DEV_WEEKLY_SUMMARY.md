# Dev Branch Weekly Summary

## Big Highlights
- Kas‑Draw runs off‑chain (instant) and on‑chain (real txids) with a clean TUI and one‑click demos.
- Program‑ID + Checkpoints: verifiable state without an L1 VM; small on‑chain anchors for public audit.
- kdapp‑indexer replaces the legacy indexer and powers Comment‑IT’s resume/feed UX.
- Core proxy listener hardened to reconnect and avoid brittle panics.

## Kas‑Draw
- New example end‑to‑end
  - 042a4cd: feat(kas-draw): L1 runner, TLV v1, watchtower trait
  - bbf06ac: feat(offchain): ACK+Close, strict seq, SimTower finalize, CLI
- Polished demos + TUI
  - 38911ec, dc1937a, 237f3b7: robust offchain_demo/onchain_demo, Windows fixes, immediate TUI render, mechanics panel
  - 17de347: fix timestamp units (ms→s) + show mechanics (draw ETA)
  - 0061b75: print state_root after init and every command (easy checkpointing)
- Program‑ID + Checkpoints
  - a0f1c1c: docs + program_id + submit_checkpoint tools; BTreeMap for deterministic state
  - 7531ec7, 143e850: record Program‑ID and add plain‑English quick start
  - f487eb6: TLV Checkpoint type + off‑chain sender + on‑chain submit‑checkpoint
  - efe2afb, 4fdd0da: compile fixes + finalized program_id tool

## Indexer + Comment‑IT
- kdapp‑indexer (new) replaces legacy indexer
  - 445f241 → b63100d → 95fe706 → 233fbd6 → 14b8c23 → f09cfce: scaffold, live listener via proxy+engine, APIs (/recent, /episode, /me), RocksDB feature, sane defaults
  - 370b57b: chore(indexer): remove comment‑it‑indexer, add kdapp‑indexer crate
- Comment‑IT integrates indexer
  - 56942b5, c9892c4, bfee024, 5db5850, 9a52012, 6f650ed, 6c1e534, 241e63a, d9334f2: feed restore, membership checks, “resume” UX, submit path stabilizations, UI polish
  - d4574be, 4500adc: docs + troubleshooting

## Core / Proxy
- 7e01139: refactor(proxy): replace asserts with warnings; more resilient wRPC listening
- Separate master PR (safe): pr/proxy‑seconds — normalize header timestamp to seconds

## Docs
- PROGRAM_ID_AND_CHECKPOINTS.md: anchor code once, verify off‑chain, checkpoint occasionally; friendly TL;DR + glossary
- PROGRAM_ID.md: recorded kas‑draw Program‑ID and how to recompute/anchor

## Demos (How to Play)
- Off‑chain: instant NEW → BUY → DRAW → CLOSE with ACKs, strict ordering, and a live dashboard
- On‑chain: real transactions with txids; tiny OKCP checkpoints for public audit

## Next (onlyKAS Merchant)
- M0: scaffold `examples/onlykas-merchant` (ReceiptEpisode + handler + SimRouter + TLV + program_id tools)
- M1: POS happy path (invoice → wallet_sim pay → receipt stored → watchtower logs)
- M2: On‑chain checkpoints via OKCP/KDCK at end‑of‑day
- M3: NFC/BLE stubs and docs

> No tokens. KAS only. L1 stays sacred; onlyKAS handles speed; checkpoints provide public verifiability.
