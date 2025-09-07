This folder contains ready-to-apply updates for the KaspaX repository.

How to apply `kaspax_comment_it_rpc_retry.patch` in the KaspaX repo:

1) In your KaspaX repo root, create a topic branch:
   git checkout -b feat/comment-it-rpc-retry

2) Copy the patch file from this repo to your KaspaX repo root, then apply it:
   git apply --index kaspax_comment_it_rpc_retry.patch

3) Build to verify:
   cargo build -p comment_it

4) Commit and push:
   git commit -m "feat(comment-it): share RPC client and retry submit; add README"
   git push -u origin feat/comment-it-rpc-retry

If `git apply` reports offsets or context issues, open the patch and apply the small hunks manually - the changes are localized to:
- applications/kdapps/comment-it/src/main.rs
- applications/kdapps/comment-it/src/cli/commands/submit_comment.rs
- applications/kdapps/comment-it/src/auth/session.rs
- applications/kdapps/comment-it/README.md (new)


Generating new patches for other examples
----------------------------------------

Use the helper script to diff each local example against the corresponding `applications/kdapps/<example>` in KaspaX and write `kaspax_<example>.patch` files here.

Prerequisites:
- Have both repos locally (this repo, and KaspaX).
- Git available on PATH.

Example usage (PowerShell):

  # From this repo root
  pwsh scripts/make_kaspax_patches.ps1 -KaspaXRoot "C:\path\to\KaspaX"

This will:
- Intersect local `examples/*` with KaspaX `applications/kdapps/*`.
- Run `git diff --no-index` for each match with cwd set to the KaspaX root.
- Emit patches as `PATCHES/kaspax_<example>.patch` targeting the KaspaX tree.

Limit to specific examples:

  pwsh scripts/make_kaspax_patches.ps1 -KaspaXRoot "C:\path\to\KaspaX" -Examples comment-board,kdapp-mcp-server

Applying a generated patch in KaspaX:

  # In KaspaX repo root
  git apply --index /path/to/this/repo/PATCHES/kaspax_<example>.patch

Notes:
- Patches only include differences; if no diff, no patch is written.
- Paths in patches are rooted at `applications/kdapps/...` so they apply cleanly from the KaspaX root.
