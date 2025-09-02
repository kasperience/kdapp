# Comment Board - Economic Episode Contract

kdapp example with economic enforcement on Kaspa L1.

## Recent Improvements

- RPC resilience: automatic reconnects in the listener and retry-on-disconnect for submissions to handle transient WebSocket drops without manual restarts.
- Non-blocking input: participant UI now renders state updates immediately while waiting for input (no aggregation or multi-comment delay).
- Organizer/participant parity: both sides reflect comments near real time even under intermittent node issues.

## Features

- **Episode Contracts** - Economic smart contracts on kdapp/Kaspa L1
- **One Episode, Many Wallets** - Shared room state, individual payments
- **Fair Economics** - You pay for your own comments, nobody can drain others
- **Kaspa Key = Identity** - No separate comment keys needed
- **Real-time Updates** - Everyone sees all comments instantly
- **Pure kdapp Architecture** - Exactly how the framework was designed
- **Unlimited Participants** - Anyone can join any room

## 📚 Documentation

**📁 Comprehensive documentation available in [`docs/`](docs/) directory:**

- **[📊 Technical Validation](docs/deepwiki-assessment.md)** - DeepWiki confirms: "Definitely doable, built on solid rusty-kaspa foundations"
- **[🗺️ Implementation Roadmap](docs/implementation-roadmap.md)** - Phase 1.1 → 1.2 → 2.0 development path  
- **[🔐 Security Analysis](docs/security-analysis.md)** - Threat model, vulnerabilities, and mitigations
- **[🏗️ Architecture Decisions](docs/architecture-decisions/)** - ADRs documenting major technical choices

**Current Status**: Phase 2.0 in progress
- Default: P2PK bond output in the combined comment transaction (standard-valid, on-chain value enforced by the episode)
- Experimental: Script-based bonds (timelock/multisig) behind `--script-bonds` flag; may be non-standard until templates are finalized

### Custom Transaction Flow (Script Bonds)
- For script-based bonds, the client assembles a raw transaction directly instead of using the standard `TransactionGenerator` helper.
- Why: Script outputs (timelock/multisig) require explicit construction and may be non-standard while templates stabilize.
- Where: `src/wallet/utxo_manager.rs` — see `submit_comment_with_bond_payload` and the Phase 2.0 helpers for script-based locking.
- The episode still validates the on-chain value and, once exposed, the script descriptor carried alongside the command.

### CLI Separation (Message vs Wallet)
- Episode commands are built as `EpisodeMessage::<ContractCommentBoard>` and routed by the generator for standard paths.
- Wallet/UTXO logic is isolated under `src/wallet/` and never mixes with episode state logic.
- This separation keeps signing/funding concerns independent from the episode’s state machine and engine.

### Advanced Commands (Optional Feature)
- Some extended contract commands are gated behind the cargo feature `advanced` to keep the example entry point small by default.
- Enable them by building or running with the feature flag:
  - Build: `cargo build -p comment-board --features advanced`
  - Run: `cargo run -p comment-board --features advanced -- --help`

### UTXO Manager Consolidation
- Historical “fix” prototypes (`wallet/utxo_manager_fix*.rs`) are now consolidated into the main `wallet/utxo_manager.rs` module.
- Use `split_large_utxo`, `ensure_micro_utxos`, and Phase 1.2/2.0 helpers in `UtxoLockManager` for current behavior.

### Dev Defaults (Registration Path)
- When an episode is registered with no participants, a deterministic, non-secret dev stub public key is used only for default initialization.
- Real rooms derive the creator from the provided participants list; no secret key is embedded or used for signing in this path.

## 🎮 Usage - Modern CLI Interface

### 🆕 Create New Room (Organizer)
```bash
# Create room with optional economic bonds
cargo run -- participant --kaspa-private-key <your-key> --bonds

# Create room with free comments (no bonds)
cargo run -- participant --kaspa-private-key <your-key>

# Output: "🚀 Creating new room with Episode ID: 123456789"
# Share this Episode ID with participants!
```

### 👥 Join Existing Room (Participants)  
```bash
# Join with economic bonds (stake 100 KAS per comment)
cargo run -- participant --kaspa-private-key <your-key> --room-episode-id 123456789 --bonds

# Join with free comments (no economic enforcement)  
cargo run -- participant --kaspa-private-key <your-key> --room-episode-id 123456789

# Your Kaspa public key becomes your username
# You pay for your own transactions, nobody can drain your wallet
```

### 🔐 Authentication Flow (Automatic)
1. **Challenge Request**: System generates random nonce
2. **Response Signing**: You sign with your Kaspa private key  
3. **Verification**: Cryptographic proof of key ownership
4. **Authenticated**: Can now post comments to the room

### 💬 Basic Interactive Commands
- **Type any text** → Submit comment to the blockchain
- **`balance`** → Check wallet status and locked bonds
- **`unlock`** → Release expired comment bonds back to wallet
- **`quit`** → Exit session (comments remain on blockchain forever)

### 🔒 **Phase 2.0 Script-Based Commands**

#### **`script-bond`** - Blockchain Script Enforcement (experimental)
```
Creates 100 KAS bond with a script-based output (timelock or multisig)
Status: experimental; may be rejected by nodes as non-standard until templates are finalized
Default bonds use P2PK output for standardness; the episode still enforces on-chain value

### Bond Output Types (Technical Overview)
- P2PK (default):
  - Why: Standard-valid across nodes; ensures smooth propagation and acceptance
  - How: Combined comment tx includes an output[0] paying back to the sender’s address for the exact `bond_amount`
  - Episode Enforcement: Validates `tx_outputs[0].value == bond_amount` for the carrier tx
  - Trade-off: Not consensus-locked; liquidity is committed at submission time, but spend policy is not enforced by script
- Script-based (experimental):
  - Why: Consensus-level enforcement (timelock / moderator multisig)
  - How: Output script encodes spend conditions; requires finalized kaspa-txscript templates for standardness
  - Status: Behind `--script-bonds`; may be rejected as non-standard by public nodes until templates are stabilized. Command carries a script descriptor; episode logs and enforces value while script verification is pending.
  - Descriptor: See `docs/script-descriptor.md` for the compact format carried on-chain and compared by the episode once tx context exposes script bytes.
```

#### **`upgrade`** - Script Migration  
```
Converts Phase 1.2 bonds to Phase 2.0 script-based enforcement
Migrates from application-trust to crypto-enforcement
Bonds become blockchain-locked UTXOs
```

#### **`bonds`** - Bond Inspector
```
Shows Phase 1.2 (App Layer) vs Phase 2.0 (Script Enforced)
Details: Script sizes, unlock conditions, confirmation status
Includes blockchain verification links
```

### 🔄 Session Flow Example
```
=== 💬 Comment Board ===
Comments: 3 | Members: 2
[1722123456] alice123: Hello everyone!
[1722123500] bob456: Hey there!
[1722123530] alice123: How's everyone doing?
========================

Enter your comment (or 'quit' to exit, 'balance' for wallet info, 'unlock' to check unlockable bonds):
```

## 🎪 How It Actually Works - CORRECT kdapp Architecture

### The Simple Truth
1. **Organizer** creates Episode with their wallet → Pays creation fee
2. **Participants** join by sending commands to same Episode ID → Each pays their own fees
3. **Everyone** uses their Kaspa key as identity → No separate keys!
4. **Episode state** is shared → All comments visible to everyone

### Real Example Flow - Fair Economics
```
Alice (Organizer): Creates Episode 12345 → Pays ~0.001 TKAS creation fee
Bob (Participant):  Joins Episode 12345 → Pays ~0.001 TKAS to join
Carol (Participant): Joins Episode 12345 → Pays ~0.001 TKAS to join  
Dave (Participant):  Joins Episode 12345 → Pays ~0.001 TKAS to join

Everyone comments: Each person pays ~0.001 TKAS per comment
Result: Alice paid ~0.001 TKAS total, not drained by others! ✅
```

## 💰 Economic Episode Contracts: Incentive-Based Economics

This implements **economic episode contracts** - the first kdapp example with voluntary economic participation:

### 🎯 **Two Usage Modes**

#### 🆓 **Free Mode** (Default)
```bash
cargo run -- participant --kaspa-private-key <key>
```
- **No bonds required** - comment freely
- **Only pay network fees** (~0.001 KAS per transaction)
- **Good for**: Testing, casual use, open discussions

#### 💎 **Economic Mode** (With `--bonds`)
```bash
cargo run -- participant --kaspa-private-key <key> --bonds
```
- **100 KAS bond per comment** - economic incentive system
- **Bonds tracked for 10 minutes** - encourages thoughtful participation  
- **Honor system unlock** - participants choose to follow economic rules
- **Good for**: High-quality discussions, preventing abuse

### 🎮 **How Incentive-Based Economics Works**

1. **Comment Submission** → 100 KAS tracked as "bonded" (application accounting)
2. **10 Minute Timer** → Encourages participants to honor the economic game
3. **Voluntary Unlock** → Use `unlock` command when timer expires
4. **Balance Tracking** → Application prevents new comments if "bonds" exceed balance

This is **economic theater** - voluntary participation in economic rules, not blockchain enforcement!

### ⚠️ **Technical Honesty: What This Is and Isn't**

**✅ What it IS:**
- Application-level economic accounting
- Incentive system encouraging good behavior
- Game theory in action - people follow rules because it benefits them
- Proof-of-concept for episode-based economics

**❌ What it ISN'T:**
- True blockchain UTXO locking (funds can be spent externally)
- Consensus-level enforcement (requires protocol changes)
- Technically preventing fund movement (only application prevents it)
- Real smart contract locking (that would need deeper integration)

**The beauty**: It works through **voluntary economic alignment**, not technical coercion!

### How the Bond is Enforced (On-Chain vs. Off-Chain State)

It's important to understand that you won't see the "bond" directly on a Kaspa block explorer. This is a key aspect of `kdapp`'s architecture:

*   **On-Chain Log:** The Kaspa blockchain records the raw transaction data (who sent what to whom). It sees that you sent a transaction with a specific payload.
*   **Off-Chain State:** The `comment-board` application (the Episode Contract) interprets that transaction payload. It understands that the payload represents a `SubmitComment` command with a `bond_amount`. The contract then updates its internal, in-memory state to reflect that your 100 KAS bond is now "locked" according to its rules.

The "lock" is a rule enforced by the contract's logic. Any attempt to spend that bonded UTXO before the 10-minute timer expires would be an invalid command according to the contract's rules, and other peers running the `kdapp` would reject it.

### Proving the Bond is Real

You can prove the bond is real by interacting with the contract's rules:

1.  **The "Negative" Proof (Attempt to break the rule):**
    *   Post a comment and immediately try to claim its bond back (you'd need to add a `ClaimBondRefund` command to the client for this).
    *   **Result:** The `execute_claim_bond_refund` function in the contract will check the time lock. Since 10 minutes haven't passed, it will fail with a `BondNotReleasable` error, proving the contract is actively enforcing the time lock.

2.  **The "Positive" Proof (Follow the rule):**
    *   Post a comment and note its `comment_id`.
    *   **Wait 10 minutes.**
    *   Then, try to claim the bond back.
    *   **Result:** The `execute_claim_bond_refund` function will succeed. The `Total Locked` value in the UI will decrease, and the UTXO associated with your bond will be released and available for you to spend.

### What if Someone Doesn't Have Enough KAS for the Bond?

This scenario is handled at two levels:

1.  **The Transaction Layer (Immediate Failure):**
    *   When you submit a comment, your client attempts to create a Kaspa transaction where a 100 KAS UTXO is effectively "bonded" (sent back to your own address, but marked as locked by the contract).
    *   If your wallet doesn't have 100 KAS (plus the small network fee) available in its UTXOs, the Kaspa network's consensus rules will reject the transaction immediately.
    *   **Result:** The transaction would fail before it's ever broadcast, and you'd see an "insufficient funds" error in your terminal.

2.  **The Episode Contract Layer (Current Demo Vulnerability):**
    *   In the current demo implementation, the contract logic in `execute_submit_comment` checks if the `bond_amount` *stated in the command payload* is 100 KAS.
    *   **Vulnerability:** This code currently *trusts* that the `bond_amount` in the command payload accurately reflects the actual KAS value locked in the underlying transaction. A malicious user could theoretically modify their client to send a command stating a 100 KAS bond, but only attach a 1 KAS UTXO to the transaction. The contract's internal state would be "fooled."

    *   **Production Fix:** A robust production system would require the `Episode::execute` function to have access to the details of the transaction that carried the command. The contract logic would then perform a direct verification: `assert_eq!(command.bond_amount, transaction.output[0].value);` This ensures the economic value stated in the command matches the economic reality on the blockchain.

## 🏗️ Why This Architecture is Perfect

- **💰 No Wallet Draining** - Each wallet pays for its own transactions
- **🔒 Cryptographically Secure** - Each comment signed with owner's key
- **⚡ Real kdapp Design** - Follows the framework exactly as intended
- **🌐 Infinite Scale** - No bottlenecks, pure blockchain coordination
- **🗳️ Permanent & Immutable** - Comments live forever on Kaspa
- **🎯 Simple Implementation** - ~300 lines total, no complex key management

## 🎯 Perfect For

- **Study Groups** - Collaborative learning with permanent records
- **Gaming Communities** - Trash talk that's provably yours!
- **Dev Teams** - Code review discussions on blockchain
- **Stream Chats** - Comment on live events, nobody can censor
- **Group Projects** - Coordination that can't be deleted by platforms

This is **pure kdapp architecture** - one episode, many wallets, everyone pays their way! 🚀
