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
#[derive(Clone, Debug, PartialEq, Eq)]
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
        info!("[ContractCommentBoard] 🚀 Episode contract initializing...");
        
        // Default contract setup - can be customized via CreateRoom command
        let default_rules = RoomRules::default();
        let creator = participants.first().copied().unwrap_or_else(|| {
            // Generate a default creator if none provided
            PubKey([0u8; 33]) // This should be replaced with actual creator
        });
        
        let contract = CommentRoomContract::new(
            creator,
            default_rules,
            vec![], // No moderators initially
            0,      // No initial funding
            Some(7776000) // 3 months default lifetime (90 days)
        );
        
        let expires_at = metadata.accepting_time + 7776000; // 3 months from creation
        
        info!("[ContractCommentBoard] ✅ Episode contract created, expires at: {}", expires_at);
        
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
                format!("🎉 Episode contract launched at block {}", metadata.accepting_daa)
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
            ContractCommand::CreateRoom { rules, moderators, initial_funding, custom_lifetime } => {
                self.execute_create_room(participant, rules, moderators, *initial_funding, 
                                       *custom_lifetime, metadata)
            }
            
            ContractCommand::JoinRoom { bond_amount } => {
                self.execute_join_room(participant, *bond_amount, metadata)
            }
            
            ContractCommand::RequestChallenge => {
                self.execute_request_challenge(participant, metadata)
            }
            
            ContractCommand::SubmitResponse { signature, nonce } => {
                self.execute_submit_response(participant, signature, nonce, metadata)
            }
            
            ContractCommand::SubmitComment { text, bond_amount } => {
                self.execute_submit_comment(participant, text, *bond_amount, metadata)
            }
            
            ContractCommand::ReportViolation { comment_id, violation_type, evidence } => {
                self.execute_report_violation(participant, *comment_id, violation_type, evidence, metadata)
            }
            
            ContractCommand::SubmitVote { vote_id, decision, stake } => {
                self.execute_submit_vote(participant, *vote_id, decision, *stake, metadata)
            }
            
            ContractCommand::ClaimBondRefund { comment_id } => {
                self.execute_claim_bond_refund(participant, *comment_id, metadata)
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
                self.locked_utxos.insert(utxo_id, (-amount) as u64);
            }
        }
        
        // Reverse reputation changes
        if let Some((user, old_rep, _new_rep)) = rollback.reputation_change {
            self.contract.reputation_scores.insert(user, old_rep);
        }
        
        // Reverse penalty pool changes
        if let Some(change) = rollback.penalty_pool_change {
            if change > 0 {
                // Money was added to pool, so remove it
                self.contract.penalty_pool = self.contract.penalty_pool.saturating_sub(change as u64);
            } else {
                // Money was removed from pool, so add it back
                self.contract.penalty_pool += (-change) as u64;
            }
        }
        
        // Specific rollback logic based on operation type
        match rollback.operation_type.as_str() {
            "submit_comment" => {
                if let Some(comment_id) = rollback.comment_id {
                    self.contract.comments.retain(|c| c.id != comment_id);
                    self.contract.comment_bonds.remove(&comment_id);
                    self.next_comment_id = comment_id; // Reset counter
                    true
                } else {
                    false
                }
            }
            "join_room" => {
                // For simplicity, we won't remove users on rollback
                // In a production system, you might want to track this more carefully
                true
            }
            _ => true, // Most operations can be rolled back successfully
        }
    }
}

impl ContractCommentBoard {
    /// Execute room creation with episode contract
    fn execute_create_room(
        &mut self,
        creator: PubKey,
        rules: &RoomRules,
        moderators: &[PubKey],
        initial_funding: u64,
        custom_lifetime: Option<u64>,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        // Update contract with provided parameters
        self.contract.room_creator = creator;
        self.contract.room_rules = rules.clone();
        self.contract.moderators = moderators.to_vec();
        self.contract.total_locked_value = initial_funding;
        self.contract.created_at = metadata.accepting_time;
        
        if let Some(lifetime) = custom_lifetime {
            self.contract_expires_at = metadata.accepting_time + lifetime;
        }
        
        // Add showcase highlight
        self.showcase_highlights.push(format!(
            "🏛️ Room created with {:.6} KAS initial funding, {} moderators",
            initial_funding as f64 / 100000000.0,
            moderators.len()
        ));
        
        info!("[ContractCommentBoard] ✅ Room created by {} with {} KAS funding", 
              creator, format_kas_amount(initial_funding));
        
        Ok(ContractRollback {
            operation_type: "create_room".to_string(),
            comment_id: None,
            bond_amount: Some(initial_funding),
            reputation_change: None,
            penalty_pool_change: None,
            prev_timestamp: metadata.accepting_time,
            utxo_changes: vec![],
        })
    }
    
    /// Execute room joining with bond requirement
    fn execute_join_room(
        &mut self,
        participant: PubKey,
        bond_amount: u64,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        let participant_str = format!("{}", participant);
        
        // Check if user can participate (reputation requirement)
        if !self.contract.can_participate(&participant) {
            let current_rep = self.contract.reputation_scores
                .get(&participant_str)
                .copied()
                .unwrap_or(0);
            return Err(EpisodeError::InvalidCommand(
                ContractError::ReputationTooLow { 
                    current: current_rep, 
                    required: self.contract.room_rules.min_reputation_threshold 
                }
            ));
        }
        
        // Check bond amount meets minimum
        let required_bond = self.contract.calculate_bond_price(&participant);
        if bond_amount < required_bond {
            return Err(EpisodeError::InvalidCommand(
                ContractError::InsufficientBond { 
                    required: required_bond, 
                    provided: bond_amount 
                }
            ));
        }
        
        // Lock the bond (in real implementation, this would be a UTXO lock)
        let utxo_id = format!("join_bond_{}_{}", participant_str, metadata.tx_id);
        self.locked_utxos.insert(utxo_id.clone(), bond_amount);
        
        // Add user to room
        self.contract.room_members.push(participant_str.clone());
        
        // Initialize reputation if first time
        if !self.contract.reputation_scores.contains_key(&participant_str) {
            self.contract.reputation_scores.insert(participant_str.clone(), 0);
        }
        
        self.contract.total_locked_value += bond_amount;
        
        info!("[ContractCommentBoard] ✅ {} joined room with {} KAS bond", 
              participant_str, format_kas_amount(bond_amount));
        
        Ok(ContractRollback {
            operation_type: "join_room".to_string(),
            comment_id: None,
            bond_amount: Some(bond_amount),
            reputation_change: None,
            penalty_pool_change: None,
            prev_timestamp: metadata.accepting_time,
            utxo_changes: vec![(utxo_id, bond_amount as i64)],
        })
    }
    
    /// Execute challenge request (existing authentication system)
    fn execute_request_challenge(
        &mut self,
        participant: PubKey,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        if self.contract.current_challenge.is_none() {
            let challenge = format!("auth_{}", metadata.tx_id);
            self.contract.current_challenge = Some(challenge.clone());
            info!("[ContractCommentBoard] 🔑 Challenge generated: {}", challenge);
        }
        
        Ok(ContractRollback {
            operation_type: "request_challenge".to_string(),
            comment_id: None,
            bond_amount: None,
            reputation_change: None,
            penalty_pool_change: None,
            prev_timestamp: metadata.accepting_time,
            utxo_changes: vec![],
        })
    }
    
    /// Execute authentication response
    fn execute_submit_response(
        &mut self,
        participant: PubKey,
        signature: &str,
        nonce: &str,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        let participant_str = format!("{}", participant);
        
        if let Some(challenge) = &self.contract.current_challenge {
            if nonce == challenge && !signature.is_empty() {
                self.contract.authenticated_users.push(participant_str.clone());
                self.contract.current_challenge = None;
                
                info!("[ContractCommentBoard] ✅ {} authenticated successfully", participant_str);
                
                Ok(ContractRollback {
                    operation_type: "authenticate".to_string(),
                    comment_id: None,
                    bond_amount: None,
                    reputation_change: None,
                    penalty_pool_change: None,
                    prev_timestamp: metadata.accepting_time,
                    utxo_changes: vec![],
                })
            } else {
                Err(EpisodeError::InvalidCommand(ContractError::UserNotAuthenticated))
            }
        } else {
            Err(EpisodeError::InvalidCommand(ContractError::UserNotAuthenticated))
        }
    }
    
    /// Execute comment submission with economic bond
    fn execute_submit_comment(
        &mut self,
        participant: PubKey,
        text: &str,
        bond_amount: u64,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        let participant_str = format!("{}", participant);
        
        // Check authentication
        if !self.contract.authenticated_users.contains(&participant_str) {
            return Err(EpisodeError::InvalidCommand(ContractError::UserNotAuthenticated));
        }
        
        // Check room membership
        if !self.contract.room_members.contains(&participant_str) {
            return Err(EpisodeError::InvalidCommand(ContractError::UserNotInRoom));
        }
        
        // Validate comment content
        if text.trim().is_empty() {
            return Err(EpisodeError::InvalidCommand(
                ContractError::RoomRulesViolation { rule: "Empty comment".to_string() }
            ));
        }
        
        if text.len() > self.contract.room_rules.max_comment_length {
            return Err(EpisodeError::InvalidCommand(
                ContractError::RoomRulesViolation { 
                    rule: format!("Comment too long (max {} chars)", self.contract.room_rules.max_comment_length)
                }
            ));
        }
        
        // Check forbidden words
        if let Some(forbidden_word) = self.contract.contains_forbidden_words(text) {
            return Err(EpisodeError::InvalidCommand(
                ContractError::RoomRulesViolation { 
                    rule: format!("Contains forbidden word: '{}'", forbidden_word)
                }
            ));
        }
        
        // Check bond amount
        let required_bond = self.contract.calculate_bond_price(&participant);
        if bond_amount < required_bond {
            return Err(EpisodeError::InvalidCommand(
                ContractError::InsufficientBond { 
                    required: required_bond, 
                    provided: bond_amount 
                }
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
        
        // Create bond tracking
        let comment_bond = CommentBond {
            comment_id,
            bonded_amount: bond_amount,
            lock_time: metadata.accepting_time + 86400, // 24 hour lock
            release_conditions: ReleaseConditions::TimeBasedRelease { 
                unlock_time: metadata.accepting_time + 86400 
            },
            utxo_reference: utxo_id.clone(),
        };
        
        // Update state
        self.contract.comments.push(economic_comment);
        self.contract.comment_bonds.insert(comment_id, comment_bond);
        self.contract.total_comments += 1;
        self.contract.total_locked_value += bond_amount;
        self.next_comment_id += 1;
        
        // Track user bonds
        self.user_bonds.entry(participant_str.clone())
            .or_insert_with(Vec::new)
            .push(comment_id);
        
        // Add showcase highlight for significant comments
        if bond_amount > self.contract.room_rules.min_bond * 2 {
            self.showcase_highlights.push(format!(
                "💰 High-value comment posted with {:.6} KAS bond",
                bond_amount as f64 / 100000000.0
            ));
        }
        
        info!("[ContractCommentBoard] ✅ Comment {} posted by {} with {} KAS bond", 
              comment_id, participant_str, format_kas_amount(bond_amount));
        
        Ok(ContractRollback {
            operation_type: "submit_comment".to_string(),
            comment_id: Some(comment_id),
            bond_amount: Some(bond_amount),
            reputation_change: None,
            penalty_pool_change: None,
            prev_timestamp: metadata.accepting_time,
            utxo_changes: vec![(utxo_id, bond_amount as i64)],
        })
    }
    
    /// Execute violation reporting
    fn execute_report_violation(
        &mut self,
        reporter: PubKey,
        comment_id: u64,
        violation_type: &ViolationType,
        evidence: &str,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        // Find the comment
        let comment_index = self.contract.comments.iter()
            .position(|c| c.id == comment_id)
            .ok_or(EpisodeError::InvalidCommand(
                ContractError::CommentNotFound { comment_id }
            ))?;
        
        // Add violation report
        let violation_report = crate::episode::contract::ViolationReport {
            reporter,
            violation_type: violation_type.clone(),
            evidence: evidence.to_string(),
            timestamp: metadata.accepting_time,
        };
        
        self.contract.comments[comment_index].reported_violations.push(violation_report);
        self.contract.comments[comment_index].moderation_status = ModerationStatus::UnderReview;
        
        info!("[ContractCommentBoard] ⚠️ Violation reported for comment {}: {:?}", 
              comment_id, violation_type);
        
        Ok(ContractRollback {
            operation_type: "report_violation".to_string(),
            comment_id: Some(comment_id),
            bond_amount: None,
            reputation_change: None,
            penalty_pool_change: None,
            prev_timestamp: metadata.accepting_time,
            utxo_changes: vec![],
        })
    }
    
    /// Execute vote submission (placeholder implementation)
    fn execute_submit_vote(
        &mut self,
        voter: PubKey,
        vote_id: u64,
        decision: &crate::episode::commands::VoteDecision,
        stake: u64,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        // This is a simplified implementation
        // Real implementation would handle complex voting logic
        
        info!("[ContractCommentBoard] 🗳️ Vote submitted by {} for vote {}: {:?} with {} KAS stake", 
              voter, vote_id, decision, format_kas_amount(stake));
        
        Ok(ContractRollback {
            operation_type: "submit_vote".to_string(),
            comment_id: None,
            bond_amount: Some(stake),
            reputation_change: None,
            penalty_pool_change: None,
            prev_timestamp: metadata.accepting_time,
            utxo_changes: vec![],
        })
    }
    
    /// Execute bond refund claim
    fn execute_claim_bond_refund(
        &mut self,
        claimer: PubKey,
        comment_id: u64,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        let claimer_str = format!("{}", claimer);
        
        // Find the comment and verify ownership
        let comment = self.contract.comments.iter()
            .find(|c| c.id == comment_id && c.author == claimer_str)
            .ok_or(EpisodeError::InvalidCommand(
                ContractError::CommentNotFound { comment_id }
            ))?;
        
        // Check if bond exists and is releasable
        let bond = self.contract.comment_bonds.get(&comment_id)
            .ok_or(EpisodeError::InvalidCommand(
                ContractError::BondNotReleasable { comment_id, unlock_time: 0 }
            ))?;
        
        // Check release conditions (simplified)
        let can_release = match &bond.release_conditions {
            ReleaseConditions::TimeBasedRelease { unlock_time } => {
                metadata.accepting_time >= *unlock_time
            }
            _ => false, // Other conditions not implemented yet
        };
        
        if !can_release {
            return Err(EpisodeError::InvalidCommand(
                ContractError::BondNotReleasable { 
                    comment_id, 
                    unlock_time: metadata.accepting_time + 86400 
                }
            ));
        }
        
        // Release the bond
        let refund_amount = bond.bonded_amount;
        self.locked_utxos.remove(&bond.utxo_reference);
        self.contract.comment_bonds.remove(&comment_id);
        self.contract.total_locked_value = self.contract.total_locked_value.saturating_sub(refund_amount);
        
        // Calculate quality bonus if applicable
        let quality_bonus = if comment.upvotes > comment.downvotes * 2 {
            refund_amount / 10 // 10% bonus for quality content
        } else {
            0
        };
        
        if quality_bonus > 0 {
            // Deduct bonus from penalty pool
            self.contract.penalty_pool = self.contract.penalty_pool.saturating_sub(quality_bonus);
        }
        
        info!("[ContractCommentBoard] ✅ Bond refund claimed: {} KAS + {} KAS quality bonus", 
              format_kas_amount(refund_amount), format_kas_amount(quality_bonus));
        
        Ok(ContractRollback {
            operation_type: "claim_bond_refund".to_string(),
            comment_id: Some(comment_id),
            bond_amount: Some(refund_amount + quality_bonus),
            reputation_change: None,
            penalty_pool_change: Some(-(quality_bonus as i64)),
            prev_timestamp: metadata.accepting_time,
            utxo_changes: vec![(bond.utxo_reference.clone(), -(refund_amount as i64))],
        })
    }
    
    /// Get contract statistics for terminal display and Twitter showcase
    pub fn get_showcase_stats(&self) -> ContractStats {
        self.contract.get_showcase_stats()
    }
    
    /// Generate terminal-friendly status display
    pub fn print_contract_status(&self) {
        println!("=== 🏛️ EPISODE CONTRACT STATUS ===");
        println!("💰 Total Locked Value: {}", format_kas_amount(self.contract.total_locked_value));
        println!("👥 Room Members: {}", self.contract.room_members.len());
        println!("💬 Total Comments: {}", self.contract.total_comments);
        println!("⚖️ Active Disputes: {}", self.contract.active_disputes);
        println!("🏆 Penalty Pool: {}", format_kas_amount(self.contract.penalty_pool));
        
        if !self.showcase_highlights.is_empty() {
            println!("\n🌟 Recent Highlights:");
            for highlight in self.showcase_highlights.iter().rev().take(3) {
                println!("   {}", highlight);
            }
        }
        
        println!("⏰ Contract Expires: {} seconds remaining", 
                 self.contract_expires_at.saturating_sub(self.contract_created_at));
        println!("===============================");
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
    pub contract_stats: ContractStats,
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
            contract_stats: self.get_showcase_stats(),
        }
    }
}