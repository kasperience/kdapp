use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use kdapp::{
    episode::{Episode, EpisodeError, PayloadMetadata},
    pki::PubKey,
};
use log::{info, warn};
use std::collections::{HashMap, HashSet};

use crate::episode::{
    contract::{CommentRoomContract, RoomRules, EconomicComment, CommentBond, ReleaseConditions, 
              ViolationType, ModerationStatus, ContractStats},
    commands::{ContractCommand, ContractError, CommandResult, format_kas_amount}
};

/// Enhanced Comment Board with Episode Contract Integration
#[derive(Clone, Debug)]
pub struct ContractCommentBoard {
    // Core Episode Contract
    pub contract: CommentRoomContract,
    
    // UTXO Locking State
    pub locked_utxos: HashMap<String, u64>, // UTXO_ID -> locked_amount
    pub user_bonds: HashMap<String, Vec<u64>>, // PubKey -> [comment_ids with bonds]
    
    // Enhanced State Management
    pub next_comment_id: u64,
    pub next_dispute_id: u64,
    pub next_vote_id: u64,
    
    // Cache for Performance
    pub user_reputation_cache: HashMap<String, (i32, u64)>, // PubKey -> (reputation, last_update)
    pub active_votes: HashMap<u64, u64>, // vote_id -> expiry_time
    
    // Episode Contract Lifetime Management
    pub contract_created_at: u64,
    pub contract_expires_at: u64,
    
    // Twitter Showcase Data
    pub showcase_highlights: Vec<String>, // Notable events for social media
}

/// Rollback Data for Episode Contract Operations
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ContractRollback {
    pub operation_type: String,
    pub comment_id: Option<u64>,
    pub bond_amount: Option<u64>,
    pub reputation_change: Option<(String, i32, i32)>, // (user, old_rep, new_rep)
    pub penalty_pool_change: Option<i64>,
    pub prev_timestamp: u64,
    pub utxo_changes: Vec<(String, u64)>, // UTXO operations to reverse
}

impl Episode for ContractCommentBoard {
    type Command = ContractCommand;
    type CommandRollback = ContractRollback;
    type CommandError = ContractError;

    fn initialize(participants: Vec<PubKey>, metadata: &PayloadMetadata) -> Self {
        info!("[ContractCommentBoard] Episode contract initializing...");
        
        // Default contract setup - can be customized via CreateRoom command
        let default_rules = RoomRules::default();
        let creator = participants.first().copied().unwrap_or_else(|| {
            // Generate a default creator if none provided
            PubKey(secp256k1::PublicKey::from_slice(&[0u8; 33]).unwrap_or_else(|_| secp256k1::PublicKey::from_secret_key(&secp256k1::SECP256K1, &secp256k1::SecretKey::from_slice(&[1u8; 32]).unwrap())))
        });
        
        let contract = CommentRoomContract::new(
            creator,
            default_rules,
            vec![], // No moderators initially
            0,      // No initial funding
            Some(7776000) // 3 months default lifetime (90 days)
        );
        
        let expires_at = metadata.accepting_time + 7776000; // 3 months from creation
        
        info!("[ContractCommentBoard] Episode contract created, expires at: {}", expires_at);
        
        Self {
            contract,
            locked_utxos: HashMap::new(),
            user_bonds: HashMap::new(),
            next_comment_id: 1,
            next_dispute_id: 1,
            next_vote_id: 1,
            user_reputation_cache: HashMap::new(),
            active_votes: HashMap::new(),
            contract_created_at: metadata.accepting_time,
            contract_expires_at: expires_at,
            showcase_highlights: vec![
                format!("Episode contract launched at block {}", metadata.accepting_daa)
            ],
        }
    }

    fn execute(
        &mut self,
        cmd: &Self::Command,
        authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) -> Result<Self::CommandRollback, EpisodeError<Self::CommandError>> {
        let Some(participant) = authorization else {
            return Err(EpisodeError::Unauthorized);
        };

        // Check if contract has expired
        if metadata.accepting_time > self.contract_expires_at {
            return Err(EpisodeError::InvalidCommand(
                ContractError::ContractExpired { episode_id: 0 } // episode_id would come from context
            ));
        }

        let participant_str = format!("{}", participant);
        info!("[ContractCommentBoard] Executing {:?} from {}", cmd, participant_str);

        match cmd {
            ContractCommand::SubmitComment { text, bond_amount } => {
                self.execute_submit_comment(participant, text, *bond_amount, metadata)
            }
            
            _ => {
                // For now, return a simple rollback for unimplemented commands
                warn!("[ContractCommentBoard] Command {:?} not yet implemented", cmd);
                Ok(ContractRollback {
                    operation_type: "unimplemented".to_string(),
                    comment_id: None,
                    bond_amount: None,
                    reputation_change: None,
                    penalty_pool_change: None,
                    prev_timestamp: metadata.accepting_time,
                    utxo_changes: Vec::new(),
                })
            }
        }
    }

    fn rollback(&mut self, rollback: Self::CommandRollback) -> bool {
        info!("[ContractCommentBoard] Rolling back operation: {}", rollback.operation_type);
        
        // Reverse UTXO changes
        for (utxo_id, amount) in rollback.utxo_changes {
            if amount > 0 {
                // This was a lock operation, so unlock it
                self.locked_utxos.remove(&utxo_id);
            } else {
                // This was an unlock operation, so re-lock it
                self.locked_utxos.insert(utxo_id, amount);
            }
        }
        
        true
    }
}

impl ContractCommentBoard {
    /// Execute comment submission with economic bond
    fn execute_submit_comment(
        &mut self,
        participant: PubKey,
        text: &str,
        bond_amount: u64,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        let participant_str = format!("{}", participant);
        
        // Validate comment content
        if text.trim().is_empty() {
            return Err(EpisodeError::InvalidCommand(
                ContractError::RoomRulesViolation { rule: "Empty comment".to_string() }
            ));
        }
        
        // Create economic comment
        let comment_id = self.next_comment_id;
        let economic_comment = EconomicComment {
            id: comment_id,
            text: text.to_string(),
            author: participant_str.clone(),
            timestamp: metadata.accepting_time,
            bond_amount,
            upvotes: 0,
            downvotes: 0,
            quality_score: 0.0,
            earned_rewards: 0,
            reported_violations: vec![],
            moderation_status: ModerationStatus::Active,
            dispute_id: None,
        };
        
        // Lock the comment bond
        let utxo_id = format!("comment_bond_{}_{}", comment_id, metadata.tx_id);
        self.locked_utxos.insert(utxo_id.clone(), bond_amount);
        
        // Update state
        self.contract.comments.push(economic_comment);
        self.contract.total_comments += 1;
        self.contract.total_locked_value += bond_amount;
        self.next_comment_id += 1;
        
        info!("[ContractCommentBoard] Comment {} posted by {} with {} KAS bond", 
              comment_id, participant_str, format_kas_amount(bond_amount));
        
        Ok(ContractRollback {
            operation_type: "submit_comment".to_string(),
            comment_id: Some(comment_id),
            bond_amount: Some(bond_amount),
            reputation_change: None,
            penalty_pool_change: None,
            prev_timestamp: metadata.accepting_time,
            utxo_changes: vec![(utxo_id, bond_amount)],
        })
    }
    
    /// Get contract statistics for terminal display and Twitter showcase
    pub fn get_showcase_stats(&self) -> ContractStats {
        self.contract.get_showcase_stats()
    }
}

/// State for external polling (compatible with existing system)
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ContractState {
    pub comments: Vec<EconomicComment>,
    pub room_members: HashSet<String>,
    pub authenticated_users: HashSet<String>,
    pub current_challenge: Option<String>,
    pub total_comments: u64,
    pub total_locked_value: u64,
    pub penalty_pool: u64,
}

impl ContractCommentBoard {
    /// Poll current state (compatible with existing event handler)
    pub fn poll(&self) -> ContractState {
        ContractState {
            comments: self.contract.comments.clone(),
            room_members: self.contract.room_members.iter().cloned().collect(),
            authenticated_users: self.contract.authenticated_users.iter().cloned().collect(),
            current_challenge: self.contract.current_challenge.clone(),
            total_comments: self.contract.total_comments,
            total_locked_value: self.contract.total_locked_value,
            penalty_pool: self.contract.penalty_pool,
        }
    }
}