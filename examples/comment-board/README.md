# Comment Board - TRUE P2P Group Chat! 🎉

**Simple architecture**: One shared episode, everyone pays for their own transactions, no wallet draining!

## 🚀 Features - The CORRECT Way!

- **🎯 One Episode, Many Wallets** - Shared room state, individual payments
- **💰 Fair Economics** - You pay for your own comments, nobody can drain others
- **🔐 Your Kaspa Key = Your Identity** - No separate comment keys needed!
- **📺 Real-time Updates** - Everyone sees all comments instantly
- **⚡ Pure kdapp Architecture** - Exactly how the framework was designed
- **🌐 Unlimited Participants** - Anyone can join any room

## 🎮 Usage - Simple & Powerful!

### 🆕 Create New Room (Organizer)
```bash
# Create room with your Kaspa wallet
cargo run --bin comment-board -- --kaspa-private-key <your-kaspa-key>

# Output: "🚀 Creating new room with Episode ID: 123456789"
# Share this Episode ID with friends!
```

### 👥 Join Existing Room (Participants)  
```bash
# Join room with your OWN Kaspa wallet (works anytime!)
cargo run --bin comment-board -- --kaspa-private-key <your-kaspa-key> --room-episode-id 123456789

# The app automatically creates a local episode to enable participation
# You pay for your own comments with your wallet
# Your Kaspa public key becomes your username
```

### 🎯 How It Works Now
- **Organizer**: Creates Episode with their wallet → Pays creation fee
- **Participants**: Join by creating local episode with same ID → Each pays their own fees
- **Everyone**: Uses their Kaspa key as identity → Comments visible to all
- **Smart Fix**: Participants can join existing rooms anytime (local episode creation)

### 💬 Party Commands
- Type any text → Comment appears for EVERYONE in the room
- Type `quit` → Leave the room (but comments stay forever!)
- Anyone can join at any time with the Episode ID

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

## 💰 Economic Incentives: The Comment Bond

To demonstrate real "skin in the game" and prevent spam, this comment board implements a simple **Comment Bond** system:

1.  **Users pay 100 KAS to post a comment.** This amount is temporarily locked.
2.  **The bond is locked for 10 minutes.** During this time, it cannot be reclaimed.
3.  **The bond is released after 10 minutes** if no issues are reported, making it available for the user again.

This creates a basic "pay-to-play" economic participation model.

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