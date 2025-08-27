# Deterministic Session Handle (kaspa-auth, P2P-friendly)

Goal: Preserve the kaspa-auth “session token” semantics (episode-scoped capability, revocable, observable), while keeping authority on‑chain and avoiding any off‑chain bearer secrets. Indexers only mirror and cache state derived from the chain.

Core idea
- Handle = H(episode_id || pubkey || auth_tx_id)
  - `auth_tx_id` is the tx id of the successful `SubmitResponse` that transitions unauthenticated → authenticated.
  - Use a cryptographic hash (e.g., BLAKE3 or SHA‑256), hex encoded.
  - The handle is deterministic, not a secret; anyone can recompute it from chain facts. It cannot be altered without a new on‑chain auth event.

Lifecycle
- Authenticate (SubmitResponse): indexers derive `handle` and persist `{episode_id, pubkey, last_auth_tx, last_auth_time, handle}` along with membership.
- Revoke (RevokeSession): membership is removed (or marked revoked); `handle` becomes None until a new `SubmitResponse` produces a new handle.
- Reorgs: kdapp engine already emits rollbacks; listener updates membership/handle accordingly.

API surface (indexer)
- `GET /index/me/{episode_id}?pubkey=` → `{ member: bool, handle: Option<String> }`
- `GET /index/members/{episode_id}` → `[ { pubkey, handle } ]`
- Optional proof endpoint for UX/auditing: `GET /index/proof/{episode_id}?pubkey=` → `{ last_auth_tx, accepting_block, time }`

UI usage
- Persist locally: `participant_pubkey`, `last_episode_id`, `last_seen_ts:{episode}`, and returned `handle`.
- On reload: load feed via `/index/episode` + `/index/comments`; call `/index/me` to restore authenticated UI and show `handle` for correlation. No need to re‑auth.
- Submitting commands: you may include `handle` for correlation; authorization remains signatures + episode logic.

Security properties
- Tamper‑evident: derived from immutable chain data; an agent cannot fabricate or alter without producing a different on‑chain tx.
- Non‑bearer: handle alone grants no rights; it’s a stable reference for coordination and UX.
- Decentralized: any peer’s indexer produces identical handles; RocksDB (or memory) is a cache.

Notes
- This model aligns with the README’s “session token” intent while avoiding off‑chain secrets.
- Current codepath can operate tokenless (membership‑only) or with this deterministic handle added; both keep authorization on‑chain.

