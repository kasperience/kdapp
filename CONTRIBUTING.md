Contributing
============

Thanks for contributing to this workspace. A few notes to keep iteration smooth.

Local development
- Format: `cargo fmt --all`
- Lint (core crates only):
  - `cargo clippy -p kdapp -p kaspa-auth --all-targets -- -D warnings`
- Lint (full workspace, excluding noisy examples during cleanup):
  - `cargo clippy --workspace --all-targets --exclude comment-it --exclude kdapp-wallet -- -D warnings`
- Tests: `cargo test --workspace`

Notes
- We use captured format strings in logs/prints (for example, `info!("Connected to {url}")`).
- Example crates may be in flux; prefer fixing Clippy warnings in the crate you touch. If you’re short on time, use the "exclude" lint command above when validating core changes.
- Do not commit secrets or RPC credentials. Use flags and environment variables as documented in the READMEs.

Submitting changes
- Keep PRs focused and small where possible.
- Include a short summary of user-visible impacts and any follow-up tasks.
