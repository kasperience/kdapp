Fast Path Testing: Stripe‑like Trio

This guide narrows workspace commands to just the Stripe‑style P2P combo:
`kdapp-merchant`, `kdapp-customer`, and `kdapp-guardian`. It reduces build time
and disk usage compared to full `--workspace --all-targets` runs.

Quick Start (Windows / PowerShell)
- Clippy: `scripts/fastpath.ps1 clippy`
- Tests: `scripts/fastpath.ps1 test`
- Build: `scripts/fastpath.ps1 build`
- Clean: `scripts/fastpath.ps1 clean`

Useful flags
- `-NoDeps`: Skip linting dependencies during clippy.
- `-Release`: Build/test in release mode.
- `-TargetDir D:\cargo-target`: Use a larger drive for build artifacts.
- `-NoDebugInfo`: Add `-C debuginfo=0` to reduce artifact size.
- `-NoIncremental`: Set `CARGO_INCREMENTAL=0` to save disk space.

Examples
- Fast lint without deps: `scripts/fastpath.ps1 clippy -NoDeps`
- Release build to separate dir:
  `scripts/fastpath.ps1 build -Release -TargetDir D:\cargo-target`
- Run lint then tests: `scripts/fastpath.ps1 all`

Equivalent raw Cargo commands
- Clippy (narrowed):
  `cargo clippy -p kdapp-merchant -p kdapp-customer -p kdapp-guardian --all-targets -- -D warnings`
- Tests (narrowed):
  `cargo test -p kdapp-merchant -p kdapp-customer -p kdapp-guardian`
- Build (narrowed):
  `cargo build -p kdapp-merchant -p kdapp-customer -p kdapp-guardian`
- Clean (narrowed):
  `cargo clean -p kdapp-merchant -p kdapp-customer -p kdapp-guardian`

Space‑saving env toggles (optional)
- PowerShell (one‑shot):
  - `$env:RUSTFLAGS="-C debuginfo=0"`
  - `$env:CARGO_INCREMENTAL="0"`
  - `$env:CARGO_TARGET_DIR="D:\\cargo-target"`

Notes
- Repository policy: Agents do not run cargo; use these commands locally.
- The trio crates live under `examples/` and rely on the root workspace for deps.
- For end‑to‑end local flows, see `examples/kdapp-merchant/tests/README.md`.

