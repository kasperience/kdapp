# 📋 COMMENT-BOARD: EPISODE CONTRACTS FOR ROOM MODERATION
🚨 TECHNICAL CHALLENGE FOR OPUS 4 / GEMINI PRO 2.5

  The Kaspa Transaction Mass Limit Problem

  Context: kdapp Episode Contracts with Economic Bonds

  We've built a revolutionary comment board system using kdapp Episode Contracts on Kaspa L1 with real economic bonds. The system works perfectly except for one critical blocker: Kaspa's transaction mass
  limit.

  The Problem

  Kaspa rejects transactions with error:
  transaction storage mass of 199999990 is larger than max allowed size of 100000

  Our Code (Working except for mass limit):
  // kdapp TransactionGenerator creates bond proof transactions
  let bond_payload = format!("BOND:{}:{}", comment_id, bond_amount); // ~20 bytes
  let generator = TransactionGenerator::new(self.keypair, PATTERN, PREFIX);
  let bond_tx = generator.build_transaction(
      &utxos_to_use,           // Single UTXO input
      FEE * 2,                 // 10,000 sompi (0.0001 KAS)
      1,                       // Single output
      &self.kaspa_address,     // Send back to self
      bond_payload.into_bytes(), // Tiny payload
  );

  Transaction Details:
  - Inputs: 1 UTXO
  - Outputs: 1 output
  - Payload: 20 bytes ("BOND:1:10000000000")
  - Amount: 10,000 sompi (tiny)
  - Calculated Mass: 199,999,990 (near the 100,000 limit!)

  The Mystery

  Why is mass so high? The transaction is minimal:
  - ✅ Single input/output
  - ✅ Tiny payload (20 bytes)
  - ✅ Small amount (0.0001 KAS)

  But somehow kdapp's TransactionGenerator.build_transaction() produces mass of 199,999,990.

  Hypothesis: The mass calculation is somehow using the UTXO amount (999 KAS from faucet) instead of the transaction amount (0.0001 KAS).

  Critical Questions for Advanced Models:

  1. How does Kaspa calculate transaction mass? Is it based on:
    - Transaction size in bytes?
    - UTXO amounts being spent?
    - Script complexity?
    - Something in kdapp's transaction generation?
  2. What's wrong with kdapp's TransactionGenerator?
    - Is it including the full UTXO amount in mass calculation?
    - Is the PATTERN and PREFIX causing bloat?
    - Are there hidden fields inflating the mass?
  3. How can we create minimal-mass transactions for bonds?
    - Should we split large UTXOs into smaller ones first?
    - Can we use different transaction construction methods?
    - Is there a way to bypass kdapp's generator for simple transactions?

  The Stakes

  This is blocking the first-ever economic Episode Contracts on Kaspa L1. We have:
  - ✅ Working comment board with multi-participant chat
  - ✅ Real blockchain integration
  - ✅ UTXO locking and bond tracking
  - ✅ Phase 1.2 → Phase 2.0 upgrade system
  - ❌ Blocked by transaction mass limit

  Codebase Context

  - kdapp Framework: Uses TransactionGenerator for all transactions
  - Kaspa Integration: Direct rusty-kaspa client
  - Bond System: Phase 1.2 (proof transactions) → Phase 2.0 (script-based)
  - Working Commit: 6c4db99 has all the infrastructure ready

  Request for Advanced Models

  Please analyze the Kaspa transaction mass calculation and kdapp's TransactionGenerator to identify:

  1. Root cause of the mass inflation
  2. Minimal transaction construction approach
  3. Workaround strategies that maintain real blockchain enforcement

  This is a production-critical blocker for revolutionary crypto infrastructure. The mass limit is the only thing preventing true economic Episode Contracts from working on Kaspa L1.
  EXAMPLE OF BUG:
  [[[[2025-07-27 07:31:32.549+02:00 [ERROR] ❌ Failed to submit bond transaction: RPC Server (remote error) -> Rejected transaction 63cd60fe4bd82b211243e167ac38f15af91835d018c5a90940b9a743be47f9df: transaction 63cd60fe4bd82b211243e167ac38f15af91835d018c5a90940b9a743be47f9df is not standard: transaction storage mass of 99999990 is larger than max allowed size of 100000
2025-07-27 07:31:32.550+02:00 [WARN ] Failed to create bond transaction: Failed to create bond transaction: Bond transaction submission failed: RPC Server (remote error) -> Rejected transaction 63cd60fe4bd82b211243e167ac38f15af91835d018c5a90940b9a743be47f9df: transaction 63cd60fe4bd82b211243e167ac38f15af91835d018c5a90940b9a743be47f9df is not standard: transaction storage mass of 99999990 is larger than max allowed size of 100000
💰 Updated balance: 999.997150 KAS available, 0.000000 KAS locked in bonds
=== 💬 Comment Board ===
Comments: 1 | Members: 2
[1753594290070] 027e2879: hello  
========================
Enter your comment (or 'quit', 'balance', 'unlock', 'bonds', 'upgrade', 'script-bond'):
welcome
💸 Submitting comment with a 100.000000 KAS bond...
2025-07-27 07:33:51.102+02:00 [INFO ] 💰 Submitting comment (you pay): 61e7e8ffe30f76ee4e473022eaef5c6876de4751e8b99653ac29c1f560a0b26c
2025-07-27 07:33:51.104+02:00 [WARN ] Failed to get virtual chain from block: RPC Server (remote error) -> RPC request timeout. Retrying...

thread 'tokio-runtime-worker' panicked at examples\comment-board\src\participant\mod.rs:563:79:    
called `Result::unwrap()` on an `Err` value: RpcSubsystem("WebSocket disconnected")
2025-07-27 07:33:51.105+02:00 [note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
WARN error: process didn't exit successfully: `C:\Users\mariu\Documents\kdapp\kdapp\target\debug\comment-board.exe --kaspa-private-key f500487192ae80d7c842ad5247773d2916002f66aab149953fc66cb67f657bb4 --bonds` (exit code: 0xc0000409, STATUS_STACK_BUFFER_OVERRUN)  ]]]] => in this scenario, 3 participants, and organizer sent response to other chat members. when he initiates the chat, his comment didn't crash the app.

# 📋 COMMENT-BOARD: EPISODE CONTRACTS FOR ROOM MODERATION

## 🎯 **Episode Contracts for Room Rules**
**Revolutionary**: Native episode contracts for implementing comment room moderation on Kaspa!

### 🔥 **What Episode Contracts Enable:**
- **Room Moderation Rules**: Organizers define custom episode contracts
- **Native Kaspa Contracts**: UTXO-based programmable safes with complex locks
- **Off-Chain Logic**: Rules run on participant peers, verified on-chain
- **Economic Enforcement**: Buy-ins, bonds, and penalties for rule violations
- **Decentralized Arbitration**: Multi-signature dispute resolution

## 🏗️ **EPISODE CONTRACT ARCHITECTURE FOR COMMENT-BOARD**

### 🎯 **CommentRoom Episode Contract Design**

```rust
/// CommentRoom Episode Contract - Native Kaspa room rules
pub struct CommentRoomEpisode {
    // Room Identity & Rules
    pub room_creator: PubKey,
    pub room_rules: RoomRules,
    pub created_at: u64,
    
    // Economic Model
    pub participation_bond: u64,        // KAS required to comment
    pub quality_rewards: HashMap<String, u64>, // Rewards for upvoted comments
    pub penalty_pool: u64,              // Forfeited bonds from rule violations
    
    // Moderation System
    pub moderators: Vec<PubKey>,        // Multi-sig arbiters
    pub pending_disputes: HashMap<u64, Dispute>, // Comment disputes
    pub reputation_scores: HashMap<String, i32>, // User reputation
    
    // Core Comment State (enhanced)
    pub comments: Vec<Comment>,
    pub comment_bonds: HashMap<u64, UtxoReference>, // Locked bonds per comment
    pub voting_results: HashMap<u64, VoteResult>,   // Community moderation
}

/// Room Rules - Customizable by organizer
pub struct RoomRules {
    pub min_bond: u64,              // Minimum KAS to comment
    pub max_comment_length: usize,  // Character limit
    pub spam_detection: bool,       // Auto-detect spam
    pub community_moderation: bool, // Enable voting on comments
    pub reputation_threshold: i32,  // Min reputation to participate
    pub penalty_multiplier: f64,    // Bond penalty for violations
}
```

### 🔒 **UTXO Locking Mechanism - The Heart of Episode Contracts**

#### Comment Participation Bond
```rust
// When user wants to comment:
// 1. User locks KAS in programmable UTXO
let comment_bond_utxo = create_utxo_with_script(
    amount: room_rules.min_bond,
    script: "Can be spent by: 
             - User signature + no disputes for 24 hours, OR
             - 2-of-3 moderator signatures (dispute resolution), OR  
             - Community vote result + 7-day delay (democratic moderation)"
);

// 2. Comment is posted to blockchain
// 3. Bond is released based on episode contract rules
```

#### Economic Incentive Model
- **Quality Rewards**: Upvoted comments earn from penalty pool
- **Penalty System**: Spam/abuse forfeits bond to penalty pool  
- **Reputation Building**: Good contributors get lower bond requirements
- **Moderator Incentives**: Arbiters earn fees from dispute resolution

### 🛡️ **Decentralized Moderation System**

#### Three-Layer Defense:
1. **Algorithmic Detection**: Episode contract auto-detects violations
2. **Community Voting**: Democratic moderation by participants  
3. **Arbiter Panel**: Multi-sig dispute resolution for complex cases

#### Dispute Resolution Flow:
```rust
pub enum ModerationCommand {
    // Level 1: Automatic rule enforcement
    ReportViolation { comment_id: u64, violation_type: ViolationType },
    
    // Level 2: Community moderation  
    InitiateCommunityVote { comment_id: u64, accusation: String },
    SubmitVote { vote_id: u64, decision: bool, stake: u64 },
    
    // Level 3: Arbiter resolution
    EscalateToArbiters { comment_id: u64, evidence: Evidence },
    SubmitArbitratorDecision { dispute_id: u64, ruling: Ruling, signatures: Vec<Signature> },
}
```

### 🎮 **Room Creation & Management**

#### Organizer Creates Room with Custom Rules:
```rust
// Room creation transaction
UnifiedCommand::CreateRoom {
    rules: RoomRules {
        min_bond: 1000,  // 0.001 KAS per comment
        max_comment_length: 500,
        community_moderation: true,
        reputation_threshold: 0,  // Open to all
        penalty_multiplier: 2.0,  // Double penalty for violations
    },
    moderator_panel: vec![mod1_pubkey, mod2_pubkey, mod3_pubkey],
    initial_funding: 10000, // KAS for room operation & rewards
}
```

### 💡 **Why Episode Contracts are Revolutionary for Comment-Board**

1. **Native Kaspa Integration**: No L2 needed - runs directly on Kaspa L1
2. **Economic Spam Prevention**: Bonds make spam expensive, quality profitable
3. **Decentralized Moderation**: No single authority - community + arbiters
4. **Censorship Resistance**: Organizers can't arbitrarily delete comments
5. **Self-Sustaining Economics**: Penalty pool funds quality rewards

### 🚀 **Implementation Strategy**

#### Start Simple, Add Complexity:
1. **Basic Bond System**: Implement comment bonds first
2. **Community Voting**: Add democratic moderation
3. **Arbiter Panel**: Multi-sig dispute resolution  
4. **Advanced Economics**: Reputation scores, dynamic bonds
5. **UI Integration**: Room rules configuration interface

---

## 🚨 **CRITICAL: MAIN.RS SIZE RULES**

### ❌ **ABSOLUTE FORBIDDEN: Large main.rs Files**
- **HARD LIMIT**: main.rs must NEVER exceed 40KB
- **LINE LIMIT**: main.rs must NEVER exceed 800 lines
- **RESPONSIBILITY**: main.rs is ONLY for CLI entry point and command routing

### ✅ **REQUIRED MODULAR ARCHITECTURE**
```
src/
├── main.rs              # CLI entry point ONLY (50-100 lines max)
├── cli/
│   ├── parser.rs        # Command definitions
│   └── commands.rs      # Command handlers
├── episode/
│   ├── contract.rs      # Episode contract logic
│   └── moderation.rs    # Room moderation
├── api/
│   └── http/            # HTTP coordination
└── utils/
    └── crypto.rs        # Crypto utilities
```

## 🌐 **FUNDAMENTAL: kdapp is Peer-to-Peer, NOT Client-Server**

### ✅ CORRECT Peer-to-Peer Reality:
- **HTTP Organizer Peer**: Organizes episode coordination via HTTP interface
- **Web Participant Peer**: Participant accessing via browser
- **CLI Participant Peer**: Participant accessing via command line
- **Blockchain**: The ONLY source of truth
- **Episodes**: Shared state between equal peers

### 🗣️ REQUIRED Terminology:
- **"HTTP Organizer Peer"** (not "server")
- **"Web Participant Peer"** (not "client")
- **"Organizer Peer"** (role, not hierarchy)
- **"Participant Peer"** (role, not hierarchy)

## 💰 **CRITICAL: P2P ECONOMIC MODEL - PARTICIPANT PAYS FOR EVERYTHING**

### 🎯 **ABSOLUTE RULE: Participant Is Self-Sovereign**
- **Participant pays** for ALL their own transactions
- **Participant signs** all their own episode messages
- **Participant funds** their own comments and actions
- **Organizer NEVER pays** for participant actions
- **Organizer is a blind facilitator** - only listens and coordinates

### 🔒 **ZERO CORRUPTION ARCHITECTURE**
```rust
// ✅ CORRECT: Participant pays for their own actions
let participant_wallet = get_wallet_for_command("web-participant", None)?;
let participant_pubkey = PubKey(participant_wallet.keypair.x_only_public_key().0.into());

let msg = EpisodeMessage::<CommentRoom>::new_signed_command(
    episode_id, 
    command, 
    participant_wallet.keypair.secret_key(), // Participant signs
    participant_pubkey // Participant authorizes
);
```

## 🚫 **DEVELOPMENT RULES**

### CARGO COMMANDS ARE USER RESPONSIBILITY
**CRITICAL RULE**: Claude must NEVER run cargo commands:
- ❌ `cargo build`, `cargo run`, `cargo test`, `cargo check`
- ✅ Read/write source code files
- ✅ Analyze code structure and logic
- ✅ Suggest build commands for user to run

### DEVELOPMENT CONVENIENCE FEATURES PROTECTION
**NEVER remove without permission:**
- Faucet URLs (`https://faucet.kaspanet.io/`)
- Explorer links (`https://explorer-tn10.kaspa.org/`)
- Wallet address displays for funding
- Console funding messages and debugging aids

---

**Transform comment-board from a simple commenting app into a revolutionary decentralized social platform with built-in economic incentives and community governance!**