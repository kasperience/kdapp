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
    pub author: String, // PubKey as string to support serialization
    pub timestamp: u64,
    pub session_token: String,
}

/// An authenticated participant in the comment room
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, PartialEq)]
pub struct AuthenticatedParticipant {
    pub pubkey: PubKey,
    pub session_token: String,
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
    
    // Multi-participant authentication
    /// Map of authenticated participants by pubkey string
    pub authenticated_participants: HashMap<String, AuthenticatedParticipant>,
    /// In-memory rate limiting: attempts per pubkey (using string representation)
    pub rate_limits: HashMap<String, u32>,
    /// All participants who can join this room (initially just creator, others can join)
    pub authorized_participants: Vec<PubKey>,
    
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
            authenticated_participants: HashMap::new(),
            rate_limits: HashMap::new(),
            authorized_participants: participants, // Anyone can join this room
            
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
                
                let participant_key = format!("{}", participant);
                
                // Check if participant is already authenticated
                if self.authenticated_participants.contains_key(&participant_key) {
                    info!("[AuthWithCommentsEpisode] Participant already authenticated: {:?}", participant);
                    return Err(EpisodeError::InvalidCommand(AuthError::AlreadyAuthenticated));
                }
                
                // Store previous state for rollback (for this specific participant)
                let previous_participant = self.authenticated_participants.get(&participant_key).cloned();
                
                // Generate new challenge with timestamp from metadata
                let new_challenge = ChallengeGenerator::generate_with_provided_timestamp(metadata.accepting_time);
                
                // Create or update participant with challenge (but not authenticated yet)
                let participant_auth = AuthenticatedParticipant {
                    pubkey: participant,
                    session_token: String::new(), // Will be set after successful authentication
                    authenticated_at: 0, // Will be set after successful authentication
                    challenge: Some(new_challenge),
                    challenge_timestamp: metadata.accepting_time,
                };
                
                self.authenticated_participants.insert(participant_key.clone(), participant_auth);
                
                // Increment rate limit
                self.increment_rate_limit(&participant);
                
                Ok(UnifiedRollback::Challenge { 
                    participant_key,
                    previous_participant
                })
            }
            
            UnifiedCommand::SubmitResponse { signature, nonce } => {
                info!("[AuthWithCommentsEpisode] SubmitResponse from: {:?}", participant);
                
                let participant_key = format!("{}", participant);
                
                // Get the participant's current authentication state
                let participant_auth = self.authenticated_participants.get(&participant_key)
                    .ok_or(EpisodeError::InvalidCommand(AuthError::ChallengeNotFound))?;
                
                // Check if already authenticated
                if !participant_auth.session_token.is_empty() {
                    return Err(EpisodeError::InvalidCommand(AuthError::AlreadyAuthenticated));
                }
                
                // Check if challenge exists and matches
                let Some(ref current_challenge) = participant_auth.challenge else {
                    return Err(EpisodeError::InvalidCommand(AuthError::ChallengeNotFound));
                };
                
                if *nonce != *current_challenge {
                    info!("[AuthWithCommentsEpisode] Challenge mismatch - received: '{}', expected: '{}'", nonce, current_challenge);
                    return Err(EpisodeError::InvalidCommand(AuthError::InvalidChallenge));
                }
                
                // Check if challenge has expired (1 hour timeout)
                if !ChallengeGenerator::is_valid(current_challenge, 3600) {
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                    info!("[AuthWithCommentsEpisode] Challenge expired: {} (current time: {})", current_challenge, now);
                    return Err(EpisodeError::InvalidCommand(AuthError::ChallengeExpired));
                }
                
                // Verify signature
                if !SignatureVerifier::verify(&participant, current_challenge, signature) {
                    return Err(EpisodeError::InvalidCommand(AuthError::SignatureVerificationFailed));
                }
                
                // Store previous state for rollback
                let previous_participant = self.authenticated_participants.get(&participant_key).cloned();
                
                // Authenticate this specific participant
                let authenticated_participant = AuthenticatedParticipant {
                    pubkey: participant,
                    session_token: self.generate_session_token_for_participant(&participant, metadata.accepting_time),
                    authenticated_at: metadata.accepting_time,
                    challenge: participant_auth.challenge.clone(), // Keep challenge for reference
                    challenge_timestamp: participant_auth.challenge_timestamp,
                };
                
                self.authenticated_participants.insert(participant_key.clone(), authenticated_participant);
                
                info!("[AuthWithCommentsEpisode] Authentication successful for: {:?}", participant);
                
                Ok(UnifiedRollback::Authentication {
                    participant_key,
                    previous_participant,
                })
            }
            
            UnifiedCommand::RevokeSession { session_token, signature } => {
                info!("[AuthWithCommentsEpisode] RevokeSession from: {:?}", participant);
                
                let participant_key = format!("{}", participant);
                
                // Get the participant's current authentication state
                let participant_auth = self.authenticated_participants.get(&participant_key)
                    .ok_or(EpisodeError::InvalidCommand(AuthError::SessionNotFound))?;
                
                // Check if session exists and matches
                if participant_auth.session_token.is_empty() {
                    return Err(EpisodeError::InvalidCommand(AuthError::SessionAlreadyRevoked));
                }
                
                if *session_token != participant_auth.session_token {
                    return Err(EpisodeError::InvalidCommand(AuthError::InvalidSessionToken));
                }
                
                // Verify signature - participant must sign their own session token to prove ownership
                if !SignatureVerifier::verify(&participant, session_token, signature) {
                    return Err(EpisodeError::InvalidCommand(AuthError::SignatureVerificationFailed));
                }
                
                // Store previous state for rollback
                let previous_participant = participant_auth.clone();
                
                // Revoke session by removing the participant from authenticated list
                self.authenticated_participants.remove(&participant_key);
                
                info!("[AuthWithCommentsEpisode] Session revoked successfully for: {:?}", participant);
                
                Ok(UnifiedRollback::SessionRevoked {
                    participant_key,
                    previous_participant,
                })
            }
            
            UnifiedCommand::SubmitComment { text, session_token } => {
                info!("[AuthWithCommentsEpisode] SubmitComment from: {:?}", participant);
                
                // Basic validation
                if text.trim().is_empty() {
                    return Err(EpisodeError::InvalidCommand(AuthError::CommentEmpty));
                }
                
                if text.len() > 2000 {
                    return Err(EpisodeError::InvalidCommand(AuthError::CommentTooLong));
                }
                
                // Validate session token - must be authenticated and token must match
                if !self.can_comment_legacy(session_token) {
                    info!("[AuthWithCommentsEpisode] Comment rejected: Invalid session token for participant");
                    return Err(EpisodeError::InvalidCommand(AuthError::InvalidSessionToken));
                }
                
                // Create new comment
                let comment = Comment {
                    id: self.next_comment_id,
                    text: text.clone(),
                    author: format!("{}", participant),
                    timestamp: metadata.accepting_time,
                    session_token: session_token.clone(),
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
            UnifiedRollback::Challenge { participant_key, previous_participant } => {
                if let Some(prev) = previous_participant {
                    self.authenticated_participants.insert(participant_key, prev);
                } else {
                    self.authenticated_participants.remove(&participant_key);
                }
                // Note: We don't rollback rate limits as they should persist
                true
            }
            UnifiedRollback::Authentication { participant_key, previous_participant } => {
                if let Some(prev) = previous_participant {
                    self.authenticated_participants.insert(participant_key, prev);
                } else {
                    self.authenticated_participants.remove(&participant_key);
                }
                true
            }
            UnifiedRollback::SessionRevoked { participant_key, previous_participant } => {
                self.authenticated_participants.insert(participant_key, previous_participant);
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

    /// Generate a new session token for a specific participant
    fn generate_session_token_for_participant(&self, participant: &PubKey, timestamp: u64) -> String {
        use rand_chacha::ChaCha8Rng;
        use rand::SeedableRng;
        use rand::Rng;
        // Use participant pubkey + timestamp for deterministic but unique session tokens
        let participant_bytes = participant.0.serialize();
        let mut seed_data = Vec::new();
        seed_data.extend_from_slice(&participant_bytes);
        seed_data.extend_from_slice(&timestamp.to_le_bytes());
        
        let seed = participant_bytes.iter().fold(timestamp, |acc, &b| acc.wrapping_add(b as u64));
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        format!("sess_{}_{}", timestamp, rng.gen::<u64>())
    }
    
    /// Get the session token for a specific participant
    pub fn get_session_token(&self, participant: &PubKey) -> Option<&String> {
        let participant_key = format!("{}", participant);
        self.authenticated_participants.get(&participant_key)
            .map(|p| &p.session_token)
            .filter(|token| !token.is_empty())
    }
    
    /// Check if a specific participant is authenticated
    pub fn is_user_authenticated(&self, participant: &PubKey) -> bool {
        let participant_key = format!("{}", participant);
        self.authenticated_participants.get(&participant_key)
            .map_or(false, |p| !p.session_token.is_empty())
    }
    
    /// Get the current challenge for a specific participant
    pub fn get_challenge(&self, participant: &PubKey) -> Option<&String> {
        let participant_key = format!("{}", participant);
        self.authenticated_participants.get(&participant_key)
            .and_then(|p| p.challenge.as_ref())
    }
    
    /// Get the room creator's public key
    pub fn get_creator(&self) -> Option<&PubKey> {
        self.creator.as_ref()
    }
    
    /// Get all authenticated participants
    pub fn get_authenticated_participants(&self) -> Vec<&AuthenticatedParticipant> {
        self.authenticated_participants.values()
            .filter(|p| !p.session_token.is_empty())
            .collect()
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
    
    /// Get comments by a specific author
    pub fn get_comments_by_author(&self, author: &str) -> Vec<&Comment> {
        self.comments.iter().filter(|c| c.author == author).collect()
    }
    
    /// Get the latest N comments
    pub fn get_latest_comments(&self, limit: usize) -> Vec<&Comment> {
        let mut comments: Vec<&Comment> = self.comments.iter().collect();
        comments.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        comments.into_iter().take(limit).collect()
    }
    
    /// Check if a specific participant can comment with their session token
    pub fn can_comment(&self, participant: &PubKey, session_token: &str) -> bool {
        let participant_key = format!("{}", participant);
        if let Some(participant_auth) = self.authenticated_participants.get(&participant_key) {
            !participant_auth.session_token.is_empty() && participant_auth.session_token == session_token
        } else {
            false
        }
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
    
    /// Legacy can_comment method for backward compatibility with existing handlers
    pub fn can_comment_legacy(&self, session_token: &str) -> bool {
        // For backward compatibility, check if any participant can comment with this token
        self.authenticated_participants.values()
            .any(|p| !p.session_token.is_empty() && p.session_token == session_token)
    }
    
    // ============ COMPATIBILITY METHODS FOR OLD API ============
    // These methods provide backward compatibility for the existing handlers
    
    /// Get the first authenticated participant's session token (for compatibility)
    pub fn session_token(&self) -> Option<String> {
        self.authenticated_participants.values()
            .find(|p| !p.session_token.is_empty())
            .map(|p| p.session_token.clone())
    }
    
    /// Check if any participant is authenticated (for compatibility)
    pub fn is_authenticated(&self) -> bool {
        self.authenticated_participants.values()
            .any(|p| !p.session_token.is_empty())
    }
    
    /// Get the first authenticated participant's challenge (for compatibility)
    pub fn challenge(&self) -> Option<String> {
        self.authenticated_participants.values()
            .find_map(|p| p.challenge.clone())
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