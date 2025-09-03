use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::{
    episode::{Episode, EpisodeError, PayloadMetadata},
    pki::PubKey,
};
use log::info;
use std::collections::HashMap;

use crate::core::{AuthCommand, AuthError, AuthRollback};
use crate::crypto::challenges::ChallengeGenerator;
use crate::crypto::signatures::SignatureVerifier;

/// Simple authentication episode for Kaspa
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq)]
pub struct SimpleAuth {
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
    /// In-memory rate limiting with decay: attempts (timestamps) per pubkey bytes
    pub rate_limits: HashMap<Vec<u8>, Vec<u64>>, // key = compressed pubkey bytes (33)
    /// Authorized participants (who can request challenges)
    pub authorized_participants: Vec<PubKey>,
}

impl Episode for SimpleAuth {
    type Command = AuthCommand;
    type CommandRollback = AuthRollback;
    type CommandError = AuthError;

    fn initialize(participants: Vec<PubKey>, metadata: &PayloadMetadata) -> Self {
        info!("[SimpleAuth] initialize: {participants:?}");
        Self {
            owner: participants.first().copied(),
            challenge: None,
            is_authenticated: false,
            session_token: None,
            challenge_timestamp: metadata.accepting_time,
            rate_limits: HashMap::new(),
            authorized_participants: participants,
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

        // Rate limiting check (with decay window)
        if self.is_rate_limited(&participant, metadata.accepting_time) {
            return Err(EpisodeError::InvalidCommand(AuthError::RateLimited));
        }

        match cmd {
            AuthCommand::RequestChallenge => {
                info!("[SimpleAuth] RequestChallenge from: {participant:?}");

                // Store previous state for rollback
                let previous_challenge = self.challenge.clone();
                let previous_timestamp = self.challenge_timestamp;

                // Generate new challenge with timestamp from metadata and additional entropy
                let mut extra = Vec::new();
                extra.extend_from_slice(&participant.0.serialize());
                // Include tx_id in textual form to avoid tight coupling to Hash internals
                extra.extend_from_slice(format!("{}", metadata.tx_id).as_bytes());
                let new_challenge = ChallengeGenerator::generate_with_entropy(metadata.accepting_time, &extra);
                self.challenge = Some(new_challenge);
                self.challenge_timestamp = metadata.accepting_time;
                self.owner = Some(participant);

                // Increment rate limit using current DAA/time as event timestamp
                self.increment_rate_limit(&participant, metadata.accepting_time);

                Ok(AuthRollback::Challenge { previous_challenge, previous_timestamp })
            }

            AuthCommand::SubmitResponse { signature, nonce } => {
                info!("[SimpleAuth] SubmitResponse from: {participant:?}");

                // Check if already authenticated
                if self.is_authenticated {
                    return Err(EpisodeError::InvalidCommand(AuthError::AlreadyAuthenticated));
                }

                // Check if challenge exists and matches
                let Some(ref current_challenge) = self.challenge else {
                    return Err(EpisodeError::InvalidCommand(AuthError::ChallengeNotFound));
                };

                if *nonce != *current_challenge {
                    info!("[SimpleAuth] Challenge mismatch - received: '{nonce}', expected: '{current_challenge}'");
                    return Err(EpisodeError::InvalidCommand(AuthError::InvalidChallenge));
                }

                // Check if challenge has expired (1 hour timeout)
                if !ChallengeGenerator::is_valid(current_challenge, 3600) {
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                    info!("[SimpleAuth] Challenge expired: {current_challenge} (current time: {now})");
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

                info!("[SimpleAuth] Authentication successful for: {participant:?}");

                Ok(AuthRollback::Authentication { previous_auth_status, previous_session_token })
            }

            AuthCommand::RevokeSession { session_token, signature } => {
                info!("[SimpleAuth] RevokeSession from: {participant:?}");

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

                info!("[SimpleAuth] Session revoked successfully for: {participant:?}");

                Ok(AuthRollback::SessionRevoked { previous_token, was_authenticated })
            }
        }
    }

    fn rollback(&mut self, rollback: Self::CommandRollback) -> bool {
        match rollback {
            AuthRollback::Challenge { previous_challenge, previous_timestamp } => {
                self.challenge = previous_challenge;
                self.challenge_timestamp = previous_timestamp;
                // Note: We don't rollback rate limits as they should persist
                true
            }
            AuthRollback::Authentication { previous_auth_status, previous_session_token } => {
                self.is_authenticated = previous_auth_status;
                self.session_token = previous_session_token;
                true
            }
            AuthRollback::SessionRevoked { previous_token, was_authenticated } => {
                self.is_authenticated = was_authenticated;
                self.session_token = Some(previous_token);
                true
            }
        }
    }
}

impl SimpleAuth {
    /// Check if a participant is rate limited
    fn is_rate_limited(&self, pubkey: &PubKey, now: u64) -> bool {
        const WINDOW_SECS: u64 = 300; // 5 minutes decay window
        const MAX_ATTEMPTS: usize = 5;
        let key = pubkey.0.serialize().to_vec();
        if let Some(attempts) = self.rate_limits.get(&key) {
            let recent = attempts.iter().filter(|&&t| now.saturating_sub(t) <= WINDOW_SECS).count();
            recent >= MAX_ATTEMPTS
        } else {
            false
        }
    }

    /// Increment rate limit counter for a participant
    fn increment_rate_limit(&mut self, pubkey: &PubKey, now: u64) {
        const WINDOW_SECS: u64 = 300;
        let key = pubkey.0.serialize().to_vec();
        let entry = self.rate_limits.entry(key).or_default();
        entry.push(now);
        // Drop old attempts outside the window to prevent unbounded growth
        entry.retain(|&t| now.saturating_sub(t) <= WINDOW_SECS);
    }

    /// Generate a new session token
    fn generate_session_token(&self) -> String {
        use rand::Rng;
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;
        use sha2::{Digest, Sha256};
        // Mix timestamp, owner pubkey bytes and current challenge text into the seed
        let mut hasher = Sha256::new();
        hasher.update(self.challenge_timestamp.to_le_bytes());
        if let Some(owner) = self.owner {
            hasher.update(owner.0.serialize());
        }
        if let Some(ref ch) = self.challenge {
            hasher.update(ch.as_bytes());
        }
        let digest = hasher.finalize();
        let mut seed_bytes = [0u8; 8];
        seed_bytes.copy_from_slice(&digest[..8]);
        let seed = u64::from_le_bytes(seed_bytes);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        format!("sess_{}", rng.gen::<u64>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdapp::pki::generate_keypair;

    #[test]
    fn test_auth_challenge_flow() {
        let ((_s1, p1), (_s2, _p2)) = (generate_keypair(), generate_keypair());
        let metadata =
            PayloadMetadata { accepting_hash: 0u64.into(), accepting_daa: 0, accepting_time: 0, tx_id: 1u64.into(), tx_outputs: None };

        let mut auth = SimpleAuth::initialize(vec![p1], &metadata);

        // Request challenge
        let rollback = auth.execute(&AuthCommand::RequestChallenge, Some(p1), &metadata).unwrap();

        assert!(auth.challenge.is_some());
        assert!(!auth.is_authenticated);

        // Test rollback
        auth.rollback(rollback);
        assert!(auth.challenge.is_none());
    }

    #[test]
    fn test_rate_limiting() {
        let ((_s1, p1), (_s2, _p2)) = (generate_keypair(), generate_keypair());
        let metadata =
            PayloadMetadata { accepting_hash: 0u64.into(), accepting_daa: 0, accepting_time: 0, tx_id: 1u64.into(), tx_outputs: None };

        let mut auth = SimpleAuth::initialize(vec![p1], &metadata);

        // Should not be rate limited initially
        assert!(!auth.is_rate_limited(&p1, 0));

        // Make 4 requests - should still work
        for _ in 0..4 {
            auth.execute(&AuthCommand::RequestChallenge, Some(p1), &metadata).unwrap();
        }
        assert!(!auth.is_rate_limited(&p1, 0));

        // 5th request should trigger rate limit
        auth.execute(&AuthCommand::RequestChallenge, Some(p1), &metadata).unwrap();
        assert!(auth.is_rate_limited(&p1, 0));

        // 6th request should be rejected
        let result = auth.execute(&AuthCommand::RequestChallenge, Some(p1), &metadata);
        assert!(result.is_err());
    }
}
