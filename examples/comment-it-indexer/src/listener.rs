use crate::models::{CommentRow, EpisodeSnapshot};
use crate::storage::{Store, StoreError};

pub async fn run(store: Store) -> Result<(), StoreError> {
    // TODO: Connect to wRPC or reuse kdapp engine events to populate the store.
    // For now, this is a stub that idles.
    let _ = store;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}

// Helper reducers (to be called from the real listener)
pub fn apply_new_episode(store: &Store, episode_id: u64, creator_pubkey: Option<String>, created_at: u64) -> Result<(), StoreError> {
    let snapshot = EpisodeSnapshot { episode_id, creator_pubkey, created_at, authenticated_count: 0 };
    store.upsert_episode(snapshot)
}

pub fn apply_auth_update(store: &Store, episode_id: u64, pubkey: &str, _authenticated: bool, _ts: u64) -> Result<(), StoreError> {
    // We only track membership set here for quick "my episodes" lookups
    store.add_membership(pubkey, episode_id)
}

pub fn apply_new_comment(store: &Store, episode_id: u64, comment_id: u64, author: String, text: String, timestamp: u64) -> Result<(), StoreError> {
    let row = CommentRow { episode_id, comment_id, author, text, timestamp };
    store.add_comment(row)
}

