use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeSnapshot {
    pub episode_id: u64,
    pub creator_pubkey: Option<String>,
    pub created_at: u64,
    pub authenticated_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentRow {
    pub episode_id: u64,
    pub comment_id: u64,
    pub author: String,
    pub text: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeDetail {
    pub snapshot: EpisodeSnapshot,
    pub recent_comments: Vec<CommentRow>,
}
