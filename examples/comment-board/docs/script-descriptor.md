# Compact Bond Script Descriptor (Draft)

Goal: a stable, compact, and episode-verifiable description of intended bond policy that can be
carried in the transaction (script bytes) and referenced in the command. Episodes compare the
on-chain descriptor with the command’s `bond_script` for integrity.

Format (bytes)
- 0x01 P2PK
  - [33] user_pubkey
- 0x02 TimeLock
  - [8] unlock_time (LE, seconds since epoch)
  - [33] user_pubkey
- 0x03 ModeratorMultisig
  - [1] required_signatures
  - [1] moderator_count
  - [33 * moderator_count] moderator_pubkeys
  - [33] user_pubkey
- 0x04 TimeOrModerator
  - [8] unlock_time (LE)
  - [1] required_signatures
  - [1] moderator_count
  - [33 * moderator_count] moderator_pubkeys
  - [33] user_pubkey

Notes
- This is a descriptor, not a final consensus script. Today, we wrap the descriptor in a
  `ScriptPublicKey` to transport it experimentally.
- Once kaspa-txscript templates for timelock/multisig are finalized and standard-valid, we will map
  descriptors 1:1 to real opcode sequences and keep this format as the episode-verifiable policy.

Verification Plan
- Framework extension: expose script bytes in `PayloadMetadata` (optional, like `tx_outputs`).
- Episode: decode descriptor from script bytes, compare against `bond_script` in the command.
- Mismatch = reject.

Migration Path
1. Descriptor only (current): episode enforces on-chain value; descriptor logged, not verified.
2. Bytes exposure: add `script_bytes` to tx context; implement decode+compare in episode.
3. Standard templates: replace wrapped-descriptor SPKs with proper kaspa-txscript outputs.
