# Documentation Cleanup Plan

This repo has multiple documentation sources (README.md files, TESTING.md, and model-specific guidance like CLAUDE.md, GEMINI.md, OPUS_*). To reduce confusion and keep docs current, here’s a pragmatic consolidation plan.

## Goals
- One authoritative quickstart per example (how to run, how to test).
- One architecture overview per example (what runs where, flows).
- Clearly mark legacy/model-generated docs as historical references.

## Steps
1. Inventory and classify
- For each example under `examples/`:
  - Keep: `README.md`, `TESTING.md` (if actionable), and any focused `docs/*` that add value.
  - Mark legacy: `CLAUDE.md`, `GEMINI.md`, `OPUS_*`, long session logs.
  - Delete only if content is fully duplicated and not referenced.

2. Promote a single README
- Each example gets a README with:
  - Run modes (WS, HTTP, CLI), exact commands.
  - RPC reliability notes (shared client + retries if present).
  - Links to `TESTING.md` for longer flows.

3. Add a “Doc Status” banner
- At top of legacy docs: add a short banner: “Legacy/archival document. Current instructions: see README.md.”

4. Cross-link examples
- From root README, link to each example’s README and note the most stable test path.

5. Ongoing hygiene
- New features must update README + TESTING before merge.
- Keep model-generated brainstorming in `docs/archive/` with date prefixes.

## Proposed next actions
- Update `examples/comment-it/README.md` (done) with WS + HTTP modes.
- Add banners to `examples/comment-it/CLAUDE.md`, `GEMINI.md`, `OPUS_*` marking them legacy.
- Repeat for `examples/comment-board/` after we stabilize options/flags.

If you’d like, I can start applying banners and pruning obvious duplicates in a follow-up branch.
