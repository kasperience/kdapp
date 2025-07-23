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