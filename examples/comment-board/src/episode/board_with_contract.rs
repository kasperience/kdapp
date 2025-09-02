#![allow(dead_code)]
use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::{
    episode::{Episode, EpisodeError, PayloadMetadata},
    pki::PubKey,
};
use sha2::{Digest, Sha256};
use log::{info, warn};
use std::collections::{HashMap, HashSet};

use crate::episode::{
    commands::{format_kas_amount, ContractCommand, ContractError},
    contract::{CommentRoomContract, ContractStats, EconomicComment, ModerationStatus, RoomRules},
};

/// Enhanced Comment Board with Episode Contract Integration
#[derive(Clone, Debug)]
pub struct ContractCommentBoard {
    // Core Episode Contract
    pub contract: CommentRoomContract,

    // UTXO Locking State
    pub locked_utxos: HashMap<String, u64>,    // UTXO_ID -> locked_amount
    pub user_bonds: HashMap<String, Vec<u64>>, // PubKey -> [comment_ids with bonds]

    // Enhanced State Management
    pub next_comment_id: u64,
    pub next_dispute_id: u64,
    pub next_vote_id: u64,

    // Cache for Performance
    pub user_reputation_cache: HashMap<String, (i32, u64)>, // PubKey -> (reputation, last_update)
    pub active_votes: HashMap<u64, u64>,                    // vote_id -> expiry_time

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
        if participants.is_empty() {
            info!("[ContractCommentBoard] Episode registration only - no state initialization");
            // Return minimal state for episode registration (participants will sync via commands)
            // Development stub creator key (deterministic, non-secret).
            // This is used ONLY for the registration-only path (no participants)
            // so that a consistent public key is available for defaults. It is
            // not used for signing or authentication and should not be treated
            // as a real account. For real rooms, the creator is taken from
            // the participants list in the normal initialization path below.
            let seed = Sha256::digest(b"comment-board-dev-stub:room_creator");
            let dev_sk = secp256k1::SecretKey::from_slice(&seed)
                .unwrap_or_else(|_| secp256k1::SecretKey::from_slice(&[1u8; 32]).unwrap());
            let dev_pk = secp256k1::PublicKey::from_secret_key(secp256k1::SECP256K1, &dev_sk);

            return Self {
                contract: CommentRoomContract::new(
                    PubKey(dev_pk),
                    RoomRules::default(),
                    vec![],
                    0,
                    Some(7776000),
                ),
                locked_utxos: HashMap::new(),
                user_bonds: HashMap::new(),
                next_comment_id: 1,
                next_dispute_id: 1,
                next_vote_id: 1,
                user_reputation_cache: HashMap::new(),
                active_votes: HashMap::new(),
                contract_created_at: metadata.accepting_time,
                contract_expires_at: std::cmp::max(metadata.accepting_time + 7776000, 2000000000), // Ensure contract doesn't expire before 2033
                showcase_highlights: vec![],
            };
        }

        info!("[ContractCommentBoard] Episode contract initializing with {} participants...", participants.len());

        // Full contract setup for organizer
        let default_rules = RoomRules::default();
        let creator = participants.first().copied().unwrap();

        let contract = CommentRoomContract::new(
            creator,
            default_rules,
            vec![],        // No moderators initially
            0,             // No initial funding
            Some(7776000), // 3 months default lifetime (90 days)
        );

        let expires_at = std::cmp::max(metadata.accepting_time + 7776000, 2000000000); // Ensure contract doesn't expire before 2033

        info!("[ContractCommentBoard] Episode contract created, expires at: {expires_at}");

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
            showcase_highlights: vec![format!("Episode contract launched at block {daa}", daa = metadata.accepting_daa)],
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
            warn!(
                "Contract expired: current_time={current}, expires_at={expires}",
                current = metadata.accepting_time,
                expires = self.contract_expires_at
            );
            return Err(EpisodeError::InvalidCommand(
                ContractError::ContractExpired { episode_id: 0 }, // TODO: Get actual episode_id from context
            ));
        }

        let participant_str = format!("{participant}");
        info!("[ContractCommentBoard] Executing {cmd:?} from {participant_str}");

        match cmd {
            ContractCommand::JoinRoom { bond_amount } => self.execute_join_room(participant, *bond_amount, metadata),

            ContractCommand::RequestChallenge => self.execute_request_challenge(participant, metadata),

            ContractCommand::SubmitResponse { signature, nonce } => {
                self.execute_submit_response(participant, signature, nonce, metadata)
            }

            ContractCommand::SubmitComment { text, bond_amount, bond_output_index, bond_script } => {
                self.execute_submit_comment(participant, text, *bond_amount, *bond_output_index, bond_script.as_ref(), metadata)
            }
            #[cfg(feature = "advanced")]
            _ => {
                // For now, return a simple rollback for unimplemented advanced commands
                warn!("[ContractCommentBoard] Command {cmd:?} not yet implemented");
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

        // Revert comment-side state when applicable
        if rollback.operation_type == "submit_comment" {
            if let Some(cid) = rollback.comment_id {
                // Remove the comment with this id if it exists
                let before = self.contract.comments.len();
                self.contract.comments.retain(|c| c.id != cid);
                if self.contract.total_comments > 0 && self.contract.comments.len() < before {
                    self.contract.total_comments -= 1;
                }
                if let Some(bond) = rollback.bond_amount {
                    self.contract.total_locked_value = self.contract.total_locked_value.saturating_sub(bond);
                }
            }
        } else if rollback.operation_type == "join_room" {
            // Best-effort revert for join: we can’t easily know which index, so convert to set-like unique list
            // and remove last entry to avoid duplication during reorgs
            if let Some(bond) = rollback.bond_amount {
                self.contract.total_locked_value = self.contract.total_locked_value.saturating_sub(bond);
            }
            // No-op for room_members here to avoid accidental removal of legitimate joins
        }

        true
    }
}

impl ContractCommentBoard {
    /// Execute room joining with bond requirement
    fn execute_join_room(
        &mut self,
        participant: PubKey,
        bond_amount: u64,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        let participant_str = format!("{participant}");

        // Add user to room
        self.contract.room_members.push(participant_str.clone());
        self.contract.total_locked_value += bond_amount;

        info!("[ContractCommentBoard] ✅ {participant_str} joined room with {} bond", format_kas_amount(bond_amount));

        Ok(ContractRollback {
            operation_type: "join_room".to_string(),
            comment_id: None,
            bond_amount: Some(bond_amount),
            reputation_change: None,
            penalty_pool_change: None,
            prev_timestamp: metadata.accepting_time,
            utxo_changes: vec![(format!("join_bond_{participant_str}"), bond_amount)],
        })
    }

    /// Execute challenge request
    fn execute_request_challenge(
        &mut self,
        _participant: PubKey,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        let challenge = format!("auth_{tx}", tx = metadata.tx_id);
        self.contract.current_challenge = Some(challenge.clone());

        info!("[ContractCommentBoard] 🔑 Challenge generated: {challenge}");

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
        let participant_str = format!("{participant}");

        if let Some(challenge) = &self.contract.current_challenge {
            if nonce == challenge && !signature.is_empty() {
                self.contract.authenticated_users.push(participant_str.clone());
                self.contract.current_challenge = None;

                info!("[ContractCommentBoard] ✅ {participant_str} authenticated successfully");

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
        bond_output_index: Option<u32>,
        bond_script: Option<&crate::episode::commands::BondScriptKind>,
        metadata: &PayloadMetadata,
    ) -> Result<ContractRollback, EpisodeError<ContractError>> {
        let participant_str = format!("{participant}");
        info!("[ContractCommentBoard] execute_submit_comment: received bond_amount = {bond_amount}");

        // Validate comment content
        if text.trim().is_empty() {
            return Err(EpisodeError::InvalidCommand(ContractError::RoomRulesViolation { rule: "Empty comment".to_string() }));
        }

        // Flexible bond enforcement - allow participant choice
        if bond_amount > 0 {
            // Participant wants to use bonds - validate the amount
            if !self.contract.room_rules.bonds_enabled {
                return Err(EpisodeError::InvalidCommand(ContractError::RoomRulesViolation {
                    rule: "Bonds are disabled for this room by organizer".to_string(),
                }));
            }

            let required_bond = self.contract.room_rules.min_bond;
            if bond_amount < required_bond {
                return Err(EpisodeError::InvalidCommand(ContractError::InsufficientBond {
                    required: required_bond,
                    provided: bond_amount,
                }));
            }

            // If available, verify the declared bond against carrier tx outputs
            if let Some(outputs) = &metadata.tx_outputs {
                if let Some(idx) = bond_output_index.map(|v| v as usize) {
                    // Exact match: required bond must equal the value at index
                    if outputs.get(idx).map(|o| o.value) != Some(bond_amount) {
                        return Err(EpisodeError::InvalidCommand(ContractError::InsufficientBond {
                            required: bond_amount,
                            provided: 0,
                        }));
                    }
                    // If script bytes are available in tx context, decode and compare to the command descriptor
                    if let Some(descriptor) = bond_script {
                        if let Some(script_bytes) = outputs.get(idx).and_then(|o| o.script_bytes.as_ref()) {
                            if let Some(onchain_desc) = crate::wallet::kaspa_scripts::decode_bond_descriptor(script_bytes) {
                                if &onchain_desc != descriptor {
                                    return Err(EpisodeError::InvalidCommand(ContractError::InvalidCommand {
                                        reason: "bond script descriptor mismatch".to_string(),
                                    }));
                                }
                            } else {
                                log::info!("[ContractCommentBoard] Script bytes present but could not decode descriptor; accepting value-verified bond");
                            }
                        } else {
                            // Placeholder: we will verify script template once metadata exposes script bytes
                            log::info!("[ContractCommentBoard] Experimental script bond descriptor present; value verified, script policy verification pending");
                        }
                    }
                } else {
                    // Fallback: accept if any output covers the bond (Phase 1.5)
                    let has_sufficient_output = outputs.iter().any(|o| o.value >= bond_amount);
                    if !has_sufficient_output {
                        return Err(EpisodeError::InvalidCommand(ContractError::InsufficientBond {
                            required: bond_amount,
                            provided: 0,
                        }));
                    }
                }
            }
        }
        // If bond_amount == 0, participant chose no bond - allow this even if bonds are enabled

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
        let utxo_id = format!("comment_bond_{comment_id}_{txid}", txid = metadata.tx_id);
        self.locked_utxos.insert(utxo_id.clone(), bond_amount);

        // Update state
        self.contract.comments.push(economic_comment);
        self.contract.total_comments += 1;
        self.contract.total_locked_value += bond_amount;
        self.next_comment_id += 1;

        info!("[ContractCommentBoard] Comment {comment_id} posted by {participant_str} with {} bond", format_kas_amount(bond_amount));

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
    pub room_rules: RoomRules,
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
            room_rules: self.contract.room_rules.clone(),
        }
    }
}
