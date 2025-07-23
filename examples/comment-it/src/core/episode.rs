use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::{
    episode::{Episode, EpisodeError, PayloadMetadata},
    pki::PubKey,
};
use log::info;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::core::{UnifiedCommand, AuthError, UnifiedRollback};
use crate::crypto::challenges::ChallengeGenerator;
use crate::crypto::signatures::SignatureVerifier;

/// A single comment stored on the blockchain
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize, PartialEq)]
pub struct Comment {
    pub id: u64,
    pub text: String,
    pub author: String, // Public key string for serialization compatibility
    pub timestamp: u64,
}

/// An authenticated participant in the comment room
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, PartialEq)]
pub struct AuthenticatedParticipant {
    pub pubkey: PubKey,
    pub authenticated_at: u64,
    pub challenge: Option<String>,
    pub challenge_timestamp: u64,
}

/// Unified authentication and comment episode for Kaspa - SHARED COMMENT ROOM
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq)]
pub struct AuthWithCommentsEpisode {
    // Comment Room Identity
    /// Creator of the comment room (first participant)
    pub creator: Option<PubKey>,
    /// Room creation timestamp
    pub created_at: u64,
    
    // Multi-participant authentication - Pure P2P  
    /// Set of authenticated public key strings (no session tokens needed!)
    pub authenticated_participants: std::collections::HashSet<String>,
    /// In-memory rate limiting: attempts per pubkey (using string representation)
    pub rate_limits: HashMap<String, u32>,
    /// All participants who can join this room (initially just creator, others can join)
    pub authorized_participants: Vec<PubKey>,
    /// Active challenges per participant (pubkey -> challenge)
    pub active_challenges: HashMap<String, String>,
    
    // Comment state (shared by all authenticated participants)
    /// All comments stored in this episode
    pub comments: Vec<Comment>,
    /// Next comment ID
    pub next_comment_id: u64,
}




impl Episode for AuthWithCommentsEpisode {
    type Command = UnifiedCommand;
    type CommandRollback = UnifiedRollback;
    type CommandError = AuthError;

    fn initialize(participants: Vec<PubKey>, metadata: &PayloadMetadata) -> Self {
        info!("[AuthWithCommentsEpisode] initialize comment room: {:?}", participants);
        Self {
            // Comment Room Identity
            creator: participants.first().copied(),
            created_at: metadata.accepting_time,
            
            // Multi-participant authentication (empty initially) 
            authenticated_participants: std::collections::HashSet::new(),
            rate_limits: HashMap::new(),
            authorized_participants: participants, // Anyone can join this room
            active_challenges: HashMap::new(), // No active challenges initially
            
            // Comment state (shared by all authenticated participants)
            comments: Vec::new(),
            next_comment_id: 1,
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

        // Check if participant is authorized
        if !self.authorized_participants.contains(&participant) {
            return Err(EpisodeError::InvalidCommand(AuthError::NotAuthorized));
        }

        // Rate limiting check
        if self.is_rate_limited(&participant) {
            return Err(EpisodeError::InvalidCommand(AuthError::RateLimited));
        }

        match cmd {
            UnifiedCommand::RequestChallenge => {
                info!("[AuthWithCommentsEpisode] RequestChallenge from: {:?}", participant);
                
                // Check if participant is already authenticated
                let participant_key = format!("{}", participant);
                if self.authenticated_participants.contains(&participant_key) {
                    info!("[AuthWithCommentsEpisode] Participant already authenticated: {:?}", participant);
                    return Err(EpisodeError::InvalidCommand(AuthError::AlreadyAuthenticated));
                }
                
                // Generate challenge for this participant
                let new_challenge = ChallengeGenerator::generate_with_provided_timestamp(metadata.accepting_time);
                
                // Store challenge for this participant so HTTP API can access it
                self.active_challenges.insert(participant_key.clone(), new_challenge.clone());
                
                info!("[AuthWithCommentsEpisode] Generated challenge {} for participant: {}", new_challenge, participant_key);
                
                // Increment rate limit
                self.increment_rate_limit(&participant);
                
                Ok(UnifiedRollback::Challenge { 
                    participant_key: format!("{}", participant),
                    previous_participant: None // Simplified rollback
                })
            }
            
            UnifiedCommand::SubmitResponse { signature, nonce } => {
                info!("[AuthWithCommentsEpisode] SubmitResponse from: {:?}", participant);
                
                // Check if already authenticated
                let participant_key = format!("{}", participant);
                if self.authenticated_participants.contains(&participant_key) {
                    return Err(EpisodeError::InvalidCommand(AuthError::AlreadyAuthenticated));
                }
                
                // Verify signature against challenge nonce
                if !SignatureVerifier::verify(&participant, nonce, signature) {
                    return Err(EpisodeError::InvalidCommand(AuthError::SignatureVerificationFailed));
                }
                
                // ✅ PURE P2P: Add public key to authenticated set (no session tokens!)
                let participant_key = format!("{}", participant);
                let was_previously_authenticated = self.authenticated_participants.contains(&participant_key);
                self.authenticated_participants.insert(participant_key.clone());
                
                // Clean up the challenge since authentication succeeded
                self.active_challenges.remove(&participant_key);
                
                info!("[AuthWithCommentsEpisode] Authentication successful for: {:?}", participant);
                
                Ok(UnifiedRollback::Authentication {
                    participant_key,
                    was_previously_authenticated,
                })
            }
            
            UnifiedCommand::RevokeSession { session_token, signature } => {
                info!("[AuthWithCommentsEpisode] RevokeSession from: {:?}", participant);
                
                // Check if participant is authenticated
                let participant_key = format!("{}", participant);
                if !self.authenticated_participants.contains(&participant_key) {
                    return Err(EpisodeError::InvalidCommand(AuthError::SessionNotFound));
                }
                
                // Verify signature - participant must prove ownership
                if !SignatureVerifier::verify(&participant, session_token, signature) {
                    return Err(EpisodeError::InvalidCommand(AuthError::SignatureVerificationFailed));
                }
                
                // ✅ PURE P2P: Remove public key from authenticated set
                let was_previously_authenticated = self.authenticated_participants.remove(&participant_key);
                
                info!("[AuthWithCommentsEpisode] Session revoked successfully for: {:?}", participant);
                
                Ok(UnifiedRollback::SessionRevoked {
                    participant_key,
                    was_previously_authenticated,
                })
            }
            
            UnifiedCommand::SubmitComment { text, .. } => {
                info!("[AuthWithCommentsEpisode] SubmitComment from: {:?}", participant);
                
                // Basic validation
                if text.trim().is_empty() {
                    return Err(EpisodeError::InvalidCommand(AuthError::CommentEmpty));
                }
                
                if text.len() > 2000 {
                    return Err(EpisodeError::InvalidCommand(AuthError::CommentTooLong));
                }
                
                // ✅ PURE P2P: Check if public key is authenticated (no session tokens!)
                let participant_key = format!("{}", participant);
                if !self.authenticated_participants.contains(&participant_key) {
                    info!("[AuthWithCommentsEpisode] Comment rejected: Participant not authenticated");
                    return Err(EpisodeError::InvalidCommand(AuthError::InvalidSessionToken));
                }
                
                // Create new comment with pure public key authentication
                let comment = Comment {
                    id: self.next_comment_id,
                    text: text.clone(),
                    author: participant_key, // Store public key string for serialization
                    timestamp: metadata.accepting_time,
                };
                
                // Store comment
                let comment_id = self.next_comment_id;
                self.comments.push(comment);
                self.next_comment_id += 1;
                
                info!("[AuthWithCommentsEpisode] ✅ Comment {} added successfully by participant {:?}", comment_id, participant);
                
                Ok(UnifiedRollback::CommentAdded { comment_id })
            }
            
        }
    }

    fn rollback(&mut self, rollback: Self::CommandRollback) -> bool {
        match rollback {
            UnifiedRollback::Challenge { participant_key, .. } => {
                // Remove the challenge that was generated
                self.active_challenges.remove(&participant_key);
                true
            }
            UnifiedRollback::Authentication { participant_key, was_previously_authenticated } => {
                if was_previously_authenticated {
                    self.authenticated_participants.insert(participant_key);
                } else {
                    self.authenticated_participants.remove(&participant_key);
                }
                true
            }
            UnifiedRollback::SessionRevoked { participant_key, was_previously_authenticated } => {
                if was_previously_authenticated {
                    self.authenticated_participants.insert(participant_key);
                }
                true
            }
            UnifiedRollback::CommentAdded { comment_id } => {
                // Remove the comment that was just added
                if let Some(pos) = self.comments.iter().position(|c| c.id == comment_id) {
                    self.comments.remove(pos);
                    self.next_comment_id = comment_id; // Reset next_id
                    true
                } else {
                    false
                }
            }
        }
    }
}

impl AuthWithCommentsEpisode {

    /// Check if a participant is rate limited
    fn is_rate_limited(&self, pubkey: &PubKey) -> bool {
        let pubkey_str = format!("{}", pubkey);
        self.rate_limits.get(&pubkey_str).map_or(false, |&attempts| attempts >= 5)
    }

    /// Increment rate limit counter for a participant
    fn increment_rate_limit(&mut self, pubkey: &PubKey) {
        let pubkey_str = format!("{}", pubkey);
        *self.rate_limits.entry(pubkey_str).or_insert(0) += 1;
    }

    /// Check if a participant is authenticated (pure P2P)
    pub fn is_participant_authenticated(&self, participant: &PubKey) -> bool {
        let participant_key = format!("{}", participant);
        self.authenticated_participants.contains(&participant_key)
    }
    
    /// Get all authenticated participants (pure P2P)
    pub fn get_authenticated_participants(&self) -> &std::collections::HashSet<String> {
        &self.authenticated_participants
    }
    
    /// Check if a specific participant is authenticated
    pub fn is_user_authenticated(&self, participant: &PubKey) -> bool {
        let participant_key = format!("{}", participant);
        self.authenticated_participants.contains(&participant_key)
    }
    
    /// Get the current challenge for a specific participant
    /// Pure P2P: Challenges are generated on-demand, no storage needed
    pub fn generate_challenge_for_participant(&self, _participant: &PubKey, timestamp: u64) -> String {
        ChallengeGenerator::generate_with_provided_timestamp(timestamp)
    }
    
    /// Get active challenge for a participant (for HTTP API)
    pub fn get_challenge_for_participant(&self, participant: &PubKey) -> Option<String> {
        let participant_key = format!("{}", participant);
        self.active_challenges.get(&participant_key).cloned()
    }
    
    /// Get the room creator's public key
    pub fn get_creator(&self) -> Option<&PubKey> {
        self.creator.as_ref()
    }
    
    /// Get authenticated participant count (pure P2P)
    pub fn get_authenticated_count(&self) -> usize {
        self.authenticated_participants.len()
    }
    
    /// Get rate limit attempts for a user
    pub fn get_rate_limit_attempts(&self, pubkey: &PubKey) -> u32 {
        let pubkey_str = format!("{}", pubkey);
        self.rate_limits.get(&pubkey_str).copied().unwrap_or(0)
    }
    
    /// Get all comments in chronological order
    pub fn get_comments(&self) -> &Vec<Comment> {
        &self.comments
    }
    
    /// Get comments by a specific author (pure P2P - using PubKey)
    pub fn get_comments_by_author(&self, author: &PubKey) -> Vec<&Comment> {
        let author_key = format!("{}", author);
        self.comments.iter().filter(|c| c.author == author_key).collect()
    }
    
    /// Get the latest N comments
    pub fn get_latest_comments(&self, limit: usize) -> Vec<&Comment> {
        let mut comments: Vec<&Comment> = self.comments.iter().collect();
        comments.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        comments.into_iter().take(limit).collect()
    }
    
    /// Pure P2P: Check if participant can comment (no session tokens!)
    pub fn can_comment(&self, participant: &PubKey) -> bool {
        let participant_key = format!("{}", participant);
        self.authenticated_participants.contains(&participant_key)
    }

    /// Generate a memorable 6-character room code from the episode ID
    pub fn generate_room_code(episode_id: u64) -> String {
        use rand_chacha::ChaCha8Rng;
        use rand::SeedableRng;
        use rand::Rng;
        let mut rng = ChaCha8Rng::seed_from_u64(episode_id);
        let charset = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // Exclude I, O, 0, 1 for clarity
        (0..6)
            .map(|_| charset.chars().nth(rng.gen_range(0..charset.len())).unwrap())
            .collect()
    }
    
    /// Legacy can_comment method for backward compatibility - REMOVED
    /// Use can_comment(participant: &PubKey) for pure P2P authentication
    pub fn can_comment_legacy(&self, _session_token: &str) -> bool {
        // Legacy method disabled - use pure public key authentication
        false
    }
    
    // ============ COMPATIBILITY METHODS FOR OLD API ============
    // These methods provide backward compatibility for the existing handlers
    
    /// Session tokens removed - use pure public key authentication
    pub fn session_token(&self) -> Option<String> {
        None // Pure P2P: No session tokens needed!
    }
    
    /// Check if any participant is authenticated (pure P2P)
    pub fn is_authenticated(&self) -> bool {
        !self.authenticated_participants.is_empty()
    }
    
    /// Challenges are generated on-demand (pure P2P)
    pub fn challenge(&self) -> Option<String> {
        // Return any active challenge (for backward compatibility with WebSocket messages)
        self.active_challenges.values().next().cloned()
    }
    
    /// Get the room creator as owner (for compatibility)
    pub fn owner(&self) -> Option<PubKey> {
        self.creator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdapp::pki::{generate_keypair, sign_message, to_message};

    #[test]
    fn test_auth_challenge_flow() {
        let ((_s1, p1), (_s2, _p2)) = (generate_keypair(), generate_keypair());
        let metadata = PayloadMetadata { 
            accepting_hash: 0u64.into(), 
            accepting_daa: 0, 
            accepting_time: 0, 
            tx_id: 1u64.into() 
        };
        
        let mut auth = AuthWithCommentsEpisode::initialize(vec![p1], &metadata);
        
        // Request challenge
        let rollback = auth.execute(
            &UnifiedCommand::RequestChallenge, 
            Some(p1), 
            &metadata
        ).unwrap();
        
        assert!(auth.challenge.is_some());
        assert!(!auth.is_authenticated);
        
        // Test rollback
        auth.rollback(rollback);
        assert!(auth.challenge.is_none());
    }

    

    #[test]
    fn test_rate_limiting() {
        let ((_s1, p1), (_s2, _p2)) = (generate_keypair(), generate_keypair());
        let metadata = PayloadMetadata { 
            accepting_hash: 0u64.into(), 
            accepting_daa: 0, 
            accepting_time: 0, 
            tx_id: 1u64.into() 
        };
        
        let mut auth = AuthWithCommentsEpisode::initialize(vec![p1], &metadata);
        
        // Should not be rate limited initially
        assert!(!auth.is_rate_limited(&p1));
        
        // Make 4 requests - should still work
        for _ in 0..4 {
            auth.execute(&UnifiedCommand::RequestChallenge, Some(p1), &metadata).unwrap();
        }
        assert!(!auth.is_rate_limited(&p1));
        
        // 5th request should trigger rate limit
        auth.execute(&UnifiedCommand::RequestChallenge, Some(p1), &metadata).unwrap();
        assert!(auth.is_rate_limited(&p1));
        
        // 6th request should be rejected
        let result = auth.execute(&UnifiedCommand::RequestChallenge, Some(p1), &metadata);
        assert!(result.is_err());
    }

    #[test]
    fn test_comment_functionality() {
        let ((_s1, p1), (_s2, _p2)) = (generate_keypair(), generate_keypair());
        let metadata = PayloadMetadata { 
            accepting_hash: 0u64.into(), 
            accepting_daa: 0, 
            accepting_time: 1234567890, 
            tx_id: 1u64.into() 
        };
        
        let mut auth = AuthWithCommentsEpisode::initialize(vec![p1], &metadata);
        
        // First authenticate
        auth.execute(&UnifiedCommand::RequestChallenge, Some(p1), &metadata).unwrap();
        auth.is_authenticated = true;
        auth.session_token = Some("sess_123".to_string());
        
        // Submit a comment
        let comment_cmd = UnifiedCommand::SubmitComment {
            text: "Hello blockchain!".to_string(),
            session_token: "sess_123".to_string(),
        };
        
        let rollback = auth.execute(&comment_cmd, Some(p1), &metadata).unwrap();
        
        assert_eq!(auth.comments.len(), 1);
        assert_eq!(auth.comments[0].text, "Hello blockchain!");
        assert_eq!(auth.comments[0].author, format!("{}", p1));
        assert_eq!(auth.comments[0].id, 1);
        assert_eq!(auth.comments[0].session_token, "sess_123");
        assert_eq!(auth.next_comment_id, 2);
        
        // Test rollback
        auth.rollback(rollback);
        assert_eq!(auth.comments.len(), 0);
        assert_eq!(auth.next_comment_id, 1);
        
        // Test comment without authentication
        auth.is_authenticated = false;
        auth.session_token = None;
        
        let result = auth.execute(&comment_cmd, Some(p1), &metadata);
        assert!(result.is_err());
    }
}