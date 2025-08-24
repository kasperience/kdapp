# Repository Guidelines

## Project Structure & Module Organization
- Root Rust workspace (`Cargo.toml`) with primary library crate in `kdapp/src` (modules: `engine`, `episode`, `generator`, `pki`, `proxy`).
- Example applications under `examples/` (e.g., `examples/tictactoe` builds the `ttt` binary; other prototypes in `examples/kaspa-auth`, `comment-board`, `kdapp-mcp-server`).
- Python tooling: `orchestrator.py` (automation pipeline) and `tui_monitor.py` (local TUI monitor). Build artifacts in `target/`.

## Architecture Overview
```
Client (CLI/UI)
   │  submits signed commands
   ▼
Generator → Kaspa Network ← Proxy (wRPC)
   │                          │ filters txs by id pattern + prefix
   ▼                          ▼
  TX with payload       Engine (per Episode type)
                               │ executes/rolls back commands
                               ▼
                      Episode + EventHandler callbacks
```
- `generator`: builds payload-carrying txs; see `PatternType` and `PrefixType`.
- `proxy`: listens via wRPC, forwards matching payloads to engines.
- `engine`: manages episode lifecycle + rollback on reorgs.

## Build, Test, and Development Commands
- Build workspace: `cargo build --workspace` — compiles all crates.
- Lint: `cargo clippy --workspace --all-targets -W warnings` — fail on warnings.
- Format: `cargo fmt --all` — applies `.rustfmt.toml` rules.
- Test: `cargo test --workspace` — runs unit/integration tests across members.
- Run Tic‑Tac‑Toe: `cargo run -p ttt -- --help` or `cargo run -p ttt -- --kaspa-private-key <hex>`.

### Local Builds Policy (Agents)
- Do not run `cargo build`, `cargo test`, `cargo run`, `cargo fmt`, or `cargo clippy` from agents.
- The user owns all cargo invocations (WSL/cross‑platform constraints). Ask the user to run commands when needed and work from their logs.
- Prefer static analysis and minimal, surgical code changes. Avoid introducing or modifying toolchain configuration.

## Examples Quickstart
- Tic‑Tac‑Toe (`examples/tictactoe`):
  - Player 1: `cargo run -p ttt -- --kaspa-private-key <hex>` (copy printed game pubkey).
  - Player 2: `cargo run -p ttt -- --kaspa-private-key <hex> --game-opponent-key <player1_pubkey>`.
  - Options: `--wrpc-url wss://host:port`, `--mainnet`. Pattern/prefix constants in `examples/tictactoe/src/main.rs`.
- Explore `examples/kaspa-auth` and `examples/comment-board` for richer flows and docs.

## Coding Style & Naming Conventions
- Rust 2021 edition; formatting governed by `.rustfmt.toml` (max width 135, shorthand inits/try). Indent with 4 spaces; no trailing whitespace.
- Use idiomatic Rust naming: `snake_case` for modules/functions, `UpperCamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for consts.
- Keep modules cohesive; public API lives in `kdapp/src/lib.rs` and submodules.
- Prefer explicit types, structured errors (`thiserror`), and `log` macros over `println!`.

## Testing Guidelines
- Place unit tests inline with modules using `#[cfg(test)] mod tests { ... }` (see examples for patterns).
- Aim for meaningful coverage of episode logic, rollback behavior, and signature verification. Run `cargo test --workspace` locally before PRs.
- For example binaries, add targeted tests where feasible and use small, deterministic inputs.

## Commit & Pull Request Guidelines
- Commit style: Conventional prefixes (`feat:`, `fix(scope):`, `docs:`, `chore:`) as seen in history.
- PRs must: describe scope and motivation, link issues, include usage notes or screenshots/log snippets when UX changes.
- Pre-submit checklist: `cargo fmt`, `cargo clippy -W warnings`, `cargo test --workspace`. Update README/example docs if behavior changes.

## Security & Configuration Tips
- Never commit private keys or RPC secrets. Use flags like `--wrpc-url wss://host:port` and test on `testnet-10` unless explicitly targeting mainnet.
- When adding listeners or generators, keep prefix/patterns unique per engine (`generator::PatternType`, `PrefixType`) and validate payload headers.

## Pointers & Further Reading
- Overview and getting started: `README.md`.
- Tic‑Tac‑Toe walkthrough: `examples/tictactoe` (see `src/main.rs`, `src/game.rs`).
- Kaspa Auth example docs: `examples/kaspa-auth/README.md`.

## Implement a New Episode (Checklist)
- Define commands/state:
  - Create a new module or example crate; implement `Episode` with your state and `Command` enum.
  - Return a rollback token from `execute` to support reorgs.
- Pick routing:
  - Choose unique `PREFIX: PrefixType` and a 10‑bit `PATTERN: PatternType`.
- Wire engine + proxy:
  - Start `Engine::<YourEpisode, Handler>::new(rx).start(vec![Handler])`.
  - Run `proxy::run_listener(kaspad, [(PREFIX,(PATTERN, tx))].into(), exit)`.
- Submit commands:
  - Use `TransactionGenerator::build_command_transaction(...)` with `EpisodeMessage::<YourEpisode>`.
- Test locally:
  - Unit test `execute/rollback` and signature verification; run `cargo test`.

Example skeleton
```rust
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
enum Command { DoThing { x: u8 } }
#[derive(Clone, borsh::BorshSerialize, borsh::BorshDeserialize)]
enum Rollback { UndoThing }
#[derive(thiserror::Error, Debug)] enum CmdErr { #[error("invalid")] Invalid }
struct MyEpisode { /* state */ }
impl kdapp::episode::Episode for MyEpisode {
  type Command = Command; type CommandRollback = Rollback; type CommandError = CmdErr;
  fn initialize(_: Vec<PubKey>, _: &PayloadMetadata) -> Self { Self{} }
  fn execute(&mut self, _c: &Command, _a: Option<PubKey>, _: &PayloadMetadata)
    -> Result<Rollback, EpisodeError<CmdErr>> { Ok(Rollback::UndoThing) }
  fn rollback(&mut self, _r: Rollback) -> bool { true }
}
```

## Glossary
- Episode: Your app’s state machine implementing `Episode`.
- Engine: Runs episodes, validates, stores rollbacks, handles reorgs.
- EventHandler: Callbacks to push updates to UIs/clients.
- Generator: Builds txs with payload header, hunts for matching IDs.
- Proxy: wRPC listener; filters txs by pattern+prefix and forwards.
- Pattern/Prefix: Lightweight routing; keep unique per episode type.
