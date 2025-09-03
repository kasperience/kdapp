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

If `git apply` reports offsets or context issues, open the patch and apply the small hunks manually — the changes are localized to:
- applications/kdapps/comment-it/src/main.rs
- applications/kdapps/comment-it/src/cli/commands/submit_comment.rs
- applications/kdapps/comment-it/src/auth/session.rs
- applications/kdapps/comment-it/README.md (new)

