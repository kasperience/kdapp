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
}

/// Unified authentication and comment episode for Kaspa
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq)]
pub struct AuthWithCommentsEpisode {
    // Auth state
    /// Owner public key (the one being authenticated)
    pub owner: Option<PubKey>,
    /// Current challenge string for authentication
    pub challenge: Option<String>,
    /// Whether the owner is authenticated
    pub is_authenticated: bool,
    /// Session token for authenticated users
    pub session_token: Option<String>,
    /// Timestamp of last challenge generation
    pub challenge_timestamp: u64,
    /// In-memory rate limiting: attempts per pubkey (using string representation)
    pub rate_limits: HashMap<String, u32>,
    /// Authorized participants (who can request challenges)
    pub authorized_participants: Vec<PubKey>,
    
    // Comment state
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
        info!("[AuthWithCommentsEpisode] initialize: {:?}", participants);
        Self {
            // Auth state
            owner: participants.first().copied(),
            challenge: None,
            is_authenticated: false,
            session_token: None,
            challenge_timestamp: metadata.accepting_time,
            rate_limits: HashMap::new(),
            authorized_participants: participants,
            
            // Comment state
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
                
                // Store previous state for rollback
                let previous_challenge = self.challenge.clone();
                let previous_timestamp = self.challenge_timestamp;
                
                // Generate new challenge with timestamp from metadata
                let new_challenge = ChallengeGenerator::generate_with_provided_timestamp(metadata.accepting_time);
                self.challenge = Some(new_challenge);
                self.challenge_timestamp = metadata.accepting_time;
                self.owner = Some(participant);
                
                // Increment rate limit
                self.increment_rate_limit(&participant);
                
                Ok(UnifiedRollback::Challenge { 
                    previous_challenge, 
                    previous_timestamp 
                })
            }
            
            UnifiedCommand::SubmitResponse { signature, nonce } => {
                info!("[AuthWithCommentsEpisode] SubmitResponse from: {:?}", participant);
                
                // Check if already authenticated
                if self.is_authenticated {
                    return Err(EpisodeError::InvalidCommand(AuthError::AlreadyAuthenticated));
                }
                
                // Check if challenge exists and matches
                let Some(ref current_challenge) = self.challenge else {
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
                let previous_auth_status = self.is_authenticated;
                let previous_session_token = self.session_token.clone();
                
                // Authenticate user
                self.is_authenticated = true;
                self.session_token = Some(self.generate_session_token());
                
                info!("[AuthWithCommentsEpisode] Authentication successful for: {:?}", participant);
                
                Ok(UnifiedRollback::Authentication {
                    previous_auth_status,
                    previous_session_token,
                })
            }
            
            UnifiedCommand::RevokeSession { session_token, signature } => {
                info!("[AuthWithCommentsEpisode] RevokeSession from: {:?}", participant);
                
                // Check if session exists and matches
                let Some(ref current_token) = self.session_token else {
                    return Err(EpisodeError::InvalidCommand(AuthError::SessionNotFound));
                };
                
                if *session_token != *current_token {
                    return Err(EpisodeError::InvalidCommand(AuthError::InvalidSessionToken));
                }
                
                // Check if already not authenticated (session already revoked)
                if !self.is_authenticated {
                    return Err(EpisodeError::InvalidCommand(AuthError::SessionAlreadyRevoked));
                }
                
                // Verify signature - participant must sign their own session token to prove ownership
                if !SignatureVerifier::verify(&participant, session_token, signature) {
                    return Err(EpisodeError::InvalidCommand(AuthError::SignatureVerificationFailed));
                }
                
                // Store previous state for rollback
                let previous_token = self.session_token.clone().unwrap();
                let was_authenticated = self.is_authenticated;
                
                // Revoke session
                self.is_authenticated = false;
                self.session_token = None;
                
                info!("[AuthWithCommentsEpisode] Session revoked successfully for: {:?}", participant);
                
                Ok(UnifiedRollback::SessionRevoked {
                    previous_token,
                    was_authenticated,
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
                if !self.can_comment(session_token) {
                    info!("[AuthWithCommentsEpisode] Comment rejected: Invalid session token");
                    return Err(EpisodeError::InvalidCommand(AuthError::InvalidSessionToken));
                }
                
                // Create new comment
                let comment = Comment {
                    id: self.next_comment_id,
                    text: text.clone(),
                    author: format!("{}", participant),
                    timestamp: metadata.accepting_time,
                };
                
                // Store comment
                let comment_id = self.next_comment_id;
                self.comments.push(comment);
                self.next_comment_id += 1;
                
                info!("[AuthWithCommentsEpisode] ✅ Comment {} added successfully", comment_id);
                
                Ok(UnifiedRollback::CommentAdded { comment_id })
            }
            
        }
    }

    fn rollback(&mut self, rollback: Self::CommandRollback) -> bool {
        match rollback {
            UnifiedRollback::Challenge { previous_challenge, previous_timestamp } => {
                self.challenge = previous_challenge;
                self.challenge_timestamp = previous_timestamp;
                // Note: We don't rollback rate limits as they should persist
                true
            }
            UnifiedRollback::Authentication { previous_auth_status, previous_session_token } => {
                self.is_authenticated = previous_auth_status;
                self.session_token = previous_session_token;
                true
            }
            UnifiedRollback::SessionRevoked { previous_token, was_authenticated } => {
                self.is_authenticated = was_authenticated;
                self.session_token = Some(previous_token);
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

    /// Generate a new session token
    fn generate_session_token(&self) -> String {
        use rand_chacha::ChaCha8Rng;
        use rand::SeedableRng;
        use rand::Rng;
        let mut rng = ChaCha8Rng::seed_from_u64(self.challenge_timestamp);
        format!("sess_{}", rng.gen::<u64>())
    }
    
    /// Get the session token for authenticated users
    pub fn get_session_token(&self) -> Option<&String> {
        self.session_token.as_ref()
    }
    
    /// Check if user is authenticated
    pub fn is_user_authenticated(&self) -> bool {
        self.is_authenticated
    }
    
    /// Get the current challenge
    pub fn get_challenge(&self) -> Option<&String> {
        self.challenge.as_ref()
    }
    
    /// Get the owner's public key
    pub fn get_owner(&self) -> Option<&PubKey> {
        self.owner.as_ref()
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
    
    /// Check if authenticated user can comment
    pub fn can_comment(&self, session_token: &str) -> bool {
        if let Some(stored_token) = &self.session_token {
            self.is_authenticated && stored_token == session_token
        } else {
            false
        }
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