#![allow(dead_code)]
use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::{
    episode::{Episode, EpisodeError, PayloadMetadata},
    pki::PubKey,
};
use log::info;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub enum CommentError {
    Empty,
    TooLong,
    NotAuthenticated,
    Unauthorized,
}

impl std::fmt::Display for CommentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommentError::Empty => write!(f, "Comment cannot be empty."),
            CommentError::TooLong => write!(f, "Comment exceeds maximum length."),
            CommentError::NotAuthenticated => write!(f, "User must authenticate first."),
            CommentError::Unauthorized => write!(f, "Unauthorized participant."),
        }
    }
}

impl std::error::Error for CommentError {}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub enum CommentCommand {
    // Authentication commands (from kaspa-auth)
    RequestChallenge,
    SubmitResponse { signature: String, nonce: String },

    // Room commands (existing)
    JoinRoom,
    SubmitComment { text: String, bond_amount: u64 },

    // Moderation command
    SetForbiddenWords { words: Vec<String> },
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct CommentRollback {
    pub command_type: String,
    pub comment_id: Option<u64>,
    pub was_authenticated: bool,
    pub prev_timestamp: u64,
}

impl CommentRollback {
    pub fn new_join(was_authenticated: bool, prev_timestamp: u64) -> Self {
        Self { command_type: "join".to_string(), comment_id: None, was_authenticated, prev_timestamp }
    }

    pub fn new_comment(comment_id: u64, prev_timestamp: u64) -> Self {
        Self { command_type: "comment".to_string(), comment_id: Some(comment_id), was_authenticated: false, prev_timestamp }
    }
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, PartialEq, Eq)]
pub struct Comment {
    pub id: u64,
    pub text: String,
    pub author: String, // PubKey as string
    pub timestamp: u64,
    pub bond_amount: u64,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct CommentState {
    pub comments: Vec<Comment>,
    pub room_members: std::collections::HashSet<String>, // PubKey strings of joined users
    pub total_comments: u64,
    pub authenticated_users: std::collections::HashSet<String>, // PubKey strings of authenticated users
    pub current_challenge: Option<String>,                      // Current authentication challenge
}

impl CommentState {
    pub fn print(&self, args: &crate::cli::Args) {
        println!("=== Comment Board ===");
        if self.comments.is_empty() {
            println!("No comments yet. Be the first to comment!");
        } else {
            for comment in &self.comments {
                if args.bonds {
                    println!(
                        "[{}] {} (Bond: {:.6} KAS): {}",
                        comment.timestamp,
                        &comment.author[..min(8, comment.author.len())], // Show first 8 chars of pubkey
                        comment.bond_amount as f64 / 100_000_000.0,
                        comment.text
                    );
                } else {
                    println!(
                        "[{}] {}: {}",
                        comment.timestamp,
                        &comment.author[..min(8, comment.author.len())], // Show first 8 chars of pubkey
                        comment.text
                    );
                }
            }
        }
        println!("Room members: {}", self.room_members.len());
        println!("Authenticated users: {}", self.authenticated_users.len());

        println!("Total comments: {}", self.total_comments);
        println!("===================");
    }
}

fn min(a: usize, b: usize) -> usize {
    if a < b {
        a
    } else {
        b
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentBoard {
    pub(crate) comments: Vec<Comment>,
    pub(crate) creator: Option<PubKey>,                         // Room creator (first participant)
    pub(crate) room_members: std::collections::HashSet<String>, // Anyone can join
    next_comment_id: u64,
    timestamp: u64,
    comment_history: VecDeque<u64>, // Track comment IDs for potential cleanup

    // Authentication state (from kaspa-auth)
    pub challenge: Option<String>,
    pub authenticated_users: std::collections::HashSet<String>, // PubKey strings of authenticated users
    pub session_tokens: std::collections::HashMap<String, String>, // PubKey -> session_token

    // Simple moderation
    pub forbidden_words: Vec<String>, // Simple word filter for organizer
}

impl Episode for CommentBoard {
    type Command = CommentCommand;
    type CommandRollback = CommentRollback;
    type CommandError = CommentError;

    fn initialize(participants: Vec<PubKey>, metadata: &PayloadMetadata) -> Self {
        info!("[CommentBoard] 🚀 Open room created! Anyone can join and comment.");
        Self {
            comments: Vec::new(),
            creator: participants.first().copied(),         // Optional creator
            room_members: std::collections::HashSet::new(), // Empty room initially
            next_comment_id: 1,
            timestamp: metadata.accepting_time,
            comment_history: VecDeque::new(),

            // Authentication state
            challenge: None,
            authenticated_users: std::collections::HashSet::new(),
            session_tokens: std::collections::HashMap::new(),

            // Simple moderation (will be set by room creator)
            forbidden_words: Vec::new(),
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

        let participant_str = format!("{participant}");
        info!("[CommentBoard] execute: {cmd:?} from {participant_str}");

        match cmd {
            CommentCommand::RequestChallenge => {
                // Generate challenge for authentication (from kaspa-auth pattern)
                if self.challenge.is_none() {
                    let challenge = format!("auth_{tx_id}", tx_id = metadata.tx_id);
                    self.challenge = Some(challenge.clone());
                    info!("[CommentBoard] 🔑 Challenge generated for {participant_str}: {challenge}");
                } else {
                    info!("[CommentBoard] 🔑 Existing challenge for {participant_str}: {}", self.challenge.as_ref().unwrap());
                }
                let old_timestamp = self.timestamp;
                self.timestamp = metadata.accepting_time;

                Ok(CommentRollback::new_join(false, old_timestamp))
            }

            CommentCommand::SubmitResponse { signature, nonce } => {
                // Verify signature against challenge (simplified - real implementation would use secp256k1)
                if let Some(challenge) = &self.challenge {
                    if nonce == challenge && !signature.is_empty() {
                        // Authentication successful
                        self.authenticated_users.insert(participant_str.clone());
                        let session_token = format!("sess_{r}", r = rand::thread_rng().gen::<u64>());
                        self.session_tokens.insert(participant_str.clone(), session_token.clone());
                        self.challenge = None; // Clear the challenge after successful authentication

                        let old_timestamp = self.timestamp;
                        self.timestamp = metadata.accepting_time;

                        info!("[CommentBoard] ✅ {participant_str} authenticated! Session: {session_token}");
                        Ok(CommentRollback::new_join(false, old_timestamp))
                    } else {
                        Err(EpisodeError::InvalidCommand(CommentError::NotAuthenticated))
                    }
                } else {
                    Err(EpisodeError::InvalidCommand(CommentError::NotAuthenticated))
                }
            }

            CommentCommand::JoinRoom => {
                // Anyone can join! No restrictions like group Snapchat
                let was_in_room = self.room_members.contains(&participant_str);
                self.room_members.insert(participant_str.clone());

                let old_timestamp = self.timestamp;
                self.timestamp = metadata.accepting_time;

                info!(
                    "[CommentBoard] 🎉 {} joined the room! ({} members)",
                    &participant_str[..min(8, participant_str.len())],
                    self.room_members.len()
                );
                Ok(CommentRollback::new_join(was_in_room, old_timestamp))
            }

            CommentCommand::SubmitComment { text, bond_amount } => {
                // Check if user is authenticated (using kaspa-auth pattern)
                if !self.authenticated_users.contains(&participant_str) {
                    return Err(EpisodeError::InvalidCommand(CommentError::NotAuthenticated));
                }

                // Check if user is in the room
                if !self.room_members.contains(&participant_str) {
                    return Err(EpisodeError::InvalidCommand(CommentError::NotAuthenticated));
                }

                // Validate comment
                if text.trim().is_empty() {
                    return Err(EpisodeError::InvalidCommand(CommentError::Empty));
                }

                if text.len() > 500 {
                    return Err(EpisodeError::InvalidCommand(CommentError::TooLong));
                }

                // Check forbidden words
                let text_lower = text.to_lowercase();
                for forbidden_word in &self.forbidden_words {
                    if text_lower.contains(&forbidden_word.to_lowercase()) {
                        return Err(EpisodeError::InvalidCommand(CommentError::Unauthorized));
                    }
                }

                // Create comment
                let comment = Comment {
                    id: self.next_comment_id,
                    text: text.clone(),
                    author: participant_str.clone(),
                    timestamp: metadata.accepting_time,
                    bond_amount: *bond_amount,
                };

                let comment_id = self.next_comment_id;
                self.comments.push(comment);
                self.comment_history.push_back(comment_id);

                // Optional: Keep only last 50 comments (similar to TTT's 6 move limit)
                if self.comment_history.len() > 50 {
                    if let Some(old_comment_id) = self.comment_history.pop_front() {
                        self.comments.retain(|c| c.id != old_comment_id);
                    }
                }

                let old_timestamp = self.timestamp;
                self.timestamp = metadata.accepting_time;
                self.next_comment_id += 1;

                info!("[CommentBoard] ✅ Comment {comment_id} added by {participant_str}");
                Ok(CommentRollback::new_comment(comment_id, old_timestamp))
            }

            CommentCommand::SetForbiddenWords { words } => {
                // Only room creator can set forbidden words
                if let Some(creator) = &self.creator {
                    if participant != *creator {
                        return Err(EpisodeError::Unauthorized);
                    }
                } else {
                    return Err(EpisodeError::Unauthorized);
                }

                let old_timestamp = self.timestamp;
                self.timestamp = metadata.accepting_time;
                self.forbidden_words = words.clone();

                info!("[CommentBoard] 🚫 Forbidden words set: {words:?}");
                Ok(CommentRollback::new_join(false, old_timestamp))
            }
        }
    }

    fn rollback(&mut self, rollback: CommentRollback) -> bool {
        match rollback.command_type.as_str() {
            "join" => {
                self.timestamp = rollback.prev_timestamp;
                // For rollback, we could remove them from room_members
                // For simplicity, we'll leave them joined on rollback
                true
            }
            "comment" => {
                if let Some(comment_id) = rollback.comment_id {
                    self.timestamp = rollback.prev_timestamp;

                    // Remove the comment
                    let initial_len = self.comments.len();
                    self.comments.retain(|c| c.id != comment_id);

                    // Remove from history
                    self.comment_history.retain(|&id| id != comment_id);

                    // Rollback next_comment_id
                    if self.next_comment_id > 1 {
                        self.next_comment_id = comment_id;
                    }

                    self.comments.len() < initial_len // Return true if we actually removed something
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl CommentBoard {
    pub fn poll(&self) -> CommentState {
        CommentState {
            comments: self.comments.clone(),
            room_members: self.room_members.clone(),
            total_comments: self.next_comment_id - 1,
            authenticated_users: self.authenticated_users.clone(),
            current_challenge: self.challenge.clone(),
        }
    }

    pub fn get_latest_comments(&self, limit: usize) -> Vec<&Comment> {
        let mut comments: Vec<&Comment> = self.comments.iter().collect();
        comments.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // Most recent first
        comments.into_iter().take(limit).collect()
    }

    pub fn is_user_in_room(&self, participant: &PubKey) -> bool {
        let participant_str = format!("{participant}");
        self.room_members.contains(&participant_str)
    }

    pub fn get_room_code(&self) -> String {
        if let Some(creator) = &self.creator {
            // Generate a memorable room code from the creator's pubkey
            let creator_str = format!("{creator}");
            let hash = creator_str.chars().take(6).collect::<String>().to_uppercase();
            format!("ROOM-{hash}")
        } else {
            "ROOM-UNKNOWN".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdapp::pki::generate_keypair;

    #[test]
    fn test_comment_authentication() {
        let ((_sk1, pk1), (_sk2, pk2)) = (generate_keypair(), generate_keypair());
        let metadata = PayloadMetadata {
            accepting_hash: 0u64.into(),
            accepting_daa: 0,
            accepting_time: 1000,
            tx_id: 1u64.into(),
            tx_outputs: None,
        };

        let mut board = CommentBoard::initialize(vec![pk1, pk2], &metadata);

        // Request a challenge, then submit a response
        let _rb1 = board.execute(&CommentCommand::RequestChallenge, Some(pk1), &metadata).unwrap();
        let challenge = board.challenge.clone().expect("challenge should be set");
        let _rb2 = board
            .execute(&CommentCommand::SubmitResponse { signature: "sig".to_string(), nonce: challenge }, Some(pk1), &metadata)
            .unwrap();
        assert!(board.authenticated_users.contains(&format!("{pk1}")));
        assert!(!board.authenticated_users.contains(&format!("{pk2}")));
    }

    #[test]
    fn test_comment_submission() {
        let ((_sk1, pk1), (_sk2, pk2)) = (generate_keypair(), generate_keypair());
        let metadata = PayloadMetadata {
            accepting_hash: 0u64.into(),
            accepting_daa: 0,
            accepting_time: 1000,
            tx_id: 1u64.into(),
            tx_outputs: None,
        };

        let mut board = CommentBoard::initialize(vec![pk1, pk2], &metadata);

        // Authenticate first (challenge + response)
        board.execute(&CommentCommand::RequestChallenge, Some(pk1), &metadata).unwrap();
        let challenge = board.challenge.clone().expect("challenge should be set");
        board
            .execute(&CommentCommand::SubmitResponse { signature: "sig".to_string(), nonce: challenge }, Some(pk1), &metadata)
            .unwrap();

        // Join room before commenting
        board.execute(&CommentCommand::JoinRoom, Some(pk1), &metadata).unwrap();

        // Submit comment
        let comment_cmd = CommentCommand::SubmitComment { text: "Hello, blockchain!".to_string(), bond_amount: 0 };

        let rollback = board.execute(&comment_cmd, Some(pk1), &metadata).unwrap();

        assert_eq!(board.comments.len(), 1);
        assert_eq!(board.comments[0].text, "Hello, blockchain!");
        assert_eq!(board.comments[0].author, format!("{pk1}"));
        assert_eq!(board.comments[0].id, 1);
        assert_eq!(board.next_comment_id, 2);

        // Test rollback
        assert!(board.rollback(rollback));
        assert_eq!(board.comments.len(), 0);
        assert_eq!(board.next_comment_id, 1);
    }

    #[test]
    fn test_comment_without_auth() {
        let ((_sk1, pk1), (_sk2, pk2)) = (generate_keypair(), generate_keypair());
        let metadata = PayloadMetadata {
            accepting_hash: 0u64.into(),
            accepting_daa: 0,
            accepting_time: 1000,
            tx_id: 1u64.into(),
            tx_outputs: None,
        };

        let mut board = CommentBoard::initialize(vec![pk1, pk2], &metadata);

        // Try to comment without authentication
        let comment_cmd = CommentCommand::SubmitComment { text: "Unauthorized comment".to_string(), bond_amount: 0 };

        let result = board.execute(&comment_cmd, Some(pk1), &metadata);
        assert!(result.is_err());

        match result {
            Err(EpisodeError::InvalidCommand(CommentError::NotAuthenticated)) => {
                // Expected error
            }
            _ => panic!("Expected NotAuthenticated error"),
        }
    }
}
