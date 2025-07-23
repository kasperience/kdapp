use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::pki::PubKey;
use crate::episode::contract::{ViolationType, VoteDecision, RoomRules};

/// Enhanced Comment Commands for Episode Contract System
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub enum ContractCommand {
    // Room Management Commands
    CreateRoom { 
        rules: RoomRules, 
        moderators: Vec<PubKey>,
        initial_funding: u64,
        custom_lifetime: Option<u64>,
    },
    
    // Basic Participation (Enhanced)
    JoinRoom { bond_amount: u64 },
    LeaveRoom { forfeit_bond: bool },
    
    // Authentication (Existing)
    RequestChallenge,
    SubmitResponse { signature: String, nonce: String },
    
    // Economic Comment System
    SubmitComment { 
        text: String, 
        bond_amount: u64,  // User-specified bond (must meet minimum)
    },
    
    // Community Moderation System
    ReportViolation { 
        comment_id: u64, 
        violation_type: ViolationType,
        evidence: String,
    },
    
    InitiateCommunityVote { 
        comment_id: u64, 
        accusation: String,
        initial_stake: u64, // KAS staked on this vote
    },
    
    SubmitVote { 
        vote_id: u64, 
        decision: VoteDecision, 
        stake: u64,   // KAS committed to this vote decision
    },
    
    // Quality Scoring System
    UpvoteComment { comment_id: u64, stake: u64 },
    DownvoteComment { comment_id: u64, stake: u64 },
    
    // Moderator Actions (Multi-sig Required)
    EscalateToArbiters { 
        comment_id: u64, 
        evidence: String,
        moderator_signature: String,
    },
    
    SubmitArbitratorDecision { 
        dispute_id: u64, 
        ruling: VoteDecision,
        reasoning: String, 
        signatures: Vec<String>, // Multi-sig from moderator panel
    },
    
    // Economic Actions
    ClaimBondRefund { comment_id: u64 },
    ClaimQualityReward { comment_id: u64 },
    WithdrawFromPenaltyPool { amount: u64 }, // For room creator/moderators
    
    // Contract Management
    UpdateRoomRules { new_rules: RoomRules, moderator_signatures: Vec<String> },
    AddModerator { new_moderator: PubKey, existing_moderator_signatures: Vec<String> },
    RemoveModerator { moderator_to_remove: PubKey, remaining_moderator_signatures: Vec<String> },
    
    // Analytics and Showcase
    GetContractStats,
    GetUserReputation { user: PubKey },
    GetReputationLeaderboard,
    
    // Emergency Functions
    EmergencyPause { moderator_signatures: Vec<String> },
    EmergencyUnpause { moderator_signatures: Vec<String> },
    ForceResolveDispute { dispute_id: u64, admin_signature: String },
}

/// Command Results for Terminal Display and Twitter Showcase
#[derive(Debug, Clone)]
pub enum CommandResult {
    // Room Creation Results
    RoomCreated { 
        episode_id: u32, 
        total_funding: u64, 
        rules_summary: String,
        twitter_showcase_url: String,
    },
    
    // Economic Results
    CommentSubmitted { 
        comment_id: u64, 
        bond_locked: u64, 
        estimated_release_time: u64,
        reputation_change: i32,
    },
    
    BondRefunded { 
        comment_id: u64, 
        amount_returned: u64, 
        quality_bonus: u64,
        new_reputation: i32,
    },
    
    // Moderation Results
    ViolationReported { 
        report_id: u64, 
        comment_id: u64, 
        estimated_resolution_time: u64,
    },
    
    VoteSubmitted { 
        vote_id: u64, 
        stake_committed: u64, 
        current_vote_tally: VoteTally,
    },
    
    DisputeResolved { 
        dispute_id: u64, 
        final_decision: VoteDecision, 
        affected_bonds: Vec<BondAdjustment>,
        penalty_pool_change: i64,
    },
    
    // Quality System Results
    QualityRewardEarned { 
        comment_id: u64, 
        reward_amount: u64, 
        reputation_boost: i32,
    },
    
    // Analytics Results
    ContractStatsSnapshot { 
        total_locked_kas: u64,
        active_disputes: u64,
        average_reputation: f64,
        showcase_summary: String,
    },
    
    ReputationUpdate { 
        user: PubKey, 
        old_reputation: i32, 
        new_reputation: i32, 
        reputation_rank: u32,
    },
    
    // Error Results
    CommandRejected { reason: String, suggested_action: String },
    InsufficientBond { required: u64, provided: u64, user_balance: u64 },
    ReputationTooLow { current: i32, required: i32, improvement_suggestions: Vec<String> },
}

/// Vote Tally for Real-time Updates
#[derive(Debug, Clone)]
pub struct VoteTally {
    pub total_stake: u64,
    pub approve_stake: u64,
    pub penalize_stake: u64,
    pub escalate_stake: u64,
    pub dismiss_stake: u64,
    pub leading_decision: VoteDecision,
    pub consensus_percentage: f64,
}

/// Bond Adjustment for Dispute Resolution
#[derive(Debug, Clone)]
pub struct BondAdjustment {
    pub comment_id: u64,
    pub user: PubKey,
    pub old_bond: u64,
    pub new_bond: u64,
    pub change_reason: String,
}

/// Error Types for Episode Contract
#[derive(Debug, Clone)]
pub enum ContractError {
    // Economic Errors
    InsufficientBond { required: u64, provided: u64 },
    BondAlreadyLocked { comment_id: u64 },
    BondNotReleasable { comment_id: u64, unlock_time: u64 },
    
    // Reputation Errors
    ReputationTooLow { current: i32, required: i32 },
    UserNotAuthenticated,
    UserNotInRoom,
    
    // Moderation Errors
    CommentNotFound { comment_id: u64 },
    DisputeNotFound { dispute_id: u64 },
    VoteAlreadySubmitted { vote_id: u64, user: PubKey },
    VotingPeriodExpired { vote_id: u64 },
    
    // Authorization Errors
    NotModerator { user: PubKey },
    InsufficientModeratorSignatures { required: u8, provided: u8 },
    InvalidSignature { signer: PubKey },
    
    // Contract State Errors
    RoomRulesViolation { rule: String },
    ContractExpired { episode_id: u32 },
    EmergencyPauseActive,
    
    // Economic State Errors
    InsufficientPenaltyPool { requested: u64, available: u64 },
    RewardAlreadyClaimed { comment_id: u64 },
    
    // System Errors
    InvalidCommand { reason: String },
    InternalError { message: String },
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractError::InsufficientBond { required, provided } => {
                write!(f, "Insufficient bond: required {:.6} KAS, provided {:.6} KAS", 
                       *required as f64 / 100_000_000.0, *provided as f64 / 100_000_000.0)
            }
            ContractError::ReputationTooLow { current, required } => {
                write!(f, "Reputation too low: {} (required: {})", current, required)
            }
            ContractError::NotModerator { user } => {
                write!(f, "User {} is not a moderator", user)
            }
            ContractError::ContractExpired { episode_id } => {
                write!(f, "Episode contract {} has expired", episode_id)
            }
            _ => write!(f, "{:?}", self),
        }
    }
}

impl std::error::Error for ContractError {}

/// Helper function to format KAS amounts for display
pub fn format_kas_amount(amount_sompis: u64) -> String {
    format!("{:.6} KAS", amount_sompis as f64 / 100000000.0)
}

/// Helper function to format reputation with color coding for terminal
pub fn format_reputation(reputation: i32) -> String {
    let color_code = match reputation {
        r if r >= 75 => "\x1b[32m",  // Green for excellent
        r if r >= 25 => "\x1b[36m",  // Cyan for good
        r if r >= -25 => "\x1b[33m", // Yellow for neutral
        r if r >= -75 => "\x1b[31m", // Red for poor
        _ => "\x1b[35m",             // Magenta for very poor
    };
    format!("{}{}★\x1b[0m", color_code, reputation)
}

/// Generate Twitter-friendly showcase message
pub fn generate_showcase_message(stats: &crate::episode::contract::ContractStats) -> String {
    format!(
        "🚀 Live #Kaspa Episode Contract Stats:\n\
         💰 {:.6} KAS locked in smart contract\n\
         👥 {} active participants\n\
         💬 {} comments with economic incentives\n\
         ⚖️ {} disputes resolved democratically\n\
         🎯 Average reputation: {:.1}★\n\
         \n\
         #DecentralizedModeration #KaspaEcosystem #Web3Social",
        stats.total_locked_kas as f64 / 100000000.0,
        stats.active_participants,
        stats.total_comments,
        stats.active_disputes,
        stats.average_reputation
    )
}