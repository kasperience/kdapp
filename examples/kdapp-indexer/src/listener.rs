#![allow(dead_code)]
use crate::models::{CommentRow, EpisodeSnapshot};
use crate::storage::{Store, StoreError};

use comment_it::core::{AuthWithCommentsEpisode, UnifiedCommand};
use comment_it::episode_runner::{AUTH_PATTERN, AUTH_PREFIX};
use kaspa_wrpc_client::prelude::NetworkId;
use kdapp::engine::{self};
use kdapp::episode::{EpisodeEventHandler, EpisodeId, PayloadMetadata};
use kdapp::pki::PubKey;
use kdapp::proxy::{self, connect_client};
use log::{info, warn};
use std::sync::{mpsc::channel, Arc};

// Main listener: wires kdapp proxy + engine and persists events to the store
pub async fn run_with_config(
    store: Store,
    network_id: NetworkId,
    wrpc_url: Option<String>,
    exit_signal: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), StoreError> {
    // Channel for engine events
    let (sender, receiver) = channel();

    // Start engine in a blocking thread with our handler
    let store_for_engine = store.clone();
    std::thread::spawn(move || {
        let mut engine = engine::Engine::<AuthWithCommentsEpisode, IndexerHandler>::new(receiver);
        let handler = IndexerHandler { store: store_for_engine };
        engine.start(vec![handler]);
    });

    // Build engine map (prefix -> (pattern, sender))
    let engines = std::iter::once((AUTH_PREFIX, (AUTH_PATTERN, sender))).collect();

    // Connect to kaspad and start proxy listener
    let kaspad = connect_client(network_id, wrpc_url).await.map_err(|_| StoreError::Internal)?;
    info!("Indexer listener connected. Following AUTH_PREFIX transactions...");
    proxy::run_listener(kaspad, engines, exit_signal).await;
    Ok(())
}

// Engine event handler that feeds the store
struct IndexerHandler {
    store: Store,
}

impl EpisodeEventHandler<AuthWithCommentsEpisode> for IndexerHandler {
    fn on_initialize(&self, episode_id: EpisodeId, episode: &AuthWithCommentsEpisode) {
        let creator_pubkey = episode.get_creator().map(|pk| format!("{pk}"));
        let created_at = episode.created_at;
        let snapshot = EpisodeSnapshot {
            episode_id: episode_id.into(),
            creator_pubkey,
            created_at,
            authenticated_count: episode.get_authenticated_count(),
        };
        if let Err(e) = self.store.upsert_episode(snapshot) {
            warn!("indexer: failed to upsert episode {episode_id}: {e:?}");
        }
    }

    fn on_command(
        &self,
        episode_id: EpisodeId,
        episode: &AuthWithCommentsEpisode,
        cmd: &UnifiedCommand,
        authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) {
        match cmd {
            UnifiedCommand::SubmitResponse { .. } => {
                if let Some(pk) = authorization {
                    // Command succeeded → participant is authenticated; track membership
                    let pk_str = format!("{pk}");
                    if let Err(e) = self.store.add_membership(&pk_str, episode_id.into()) {
                        warn!("indexer: failed to add membership for ep {episode_id}: {e:?}");
                    }
                    // Also update snapshot with new authenticated count
                    let snapshot = EpisodeSnapshot {
                        episode_id: episode_id.into(),
                        creator_pubkey: episode.get_creator().map(|p| format!("{p}")),
                        created_at: episode.created_at,
                        authenticated_count: episode.get_authenticated_count(),
                    };
                    if let Err(e) = self.store.upsert_episode(snapshot) {
                        warn!("indexer: failed to refresh episode {episode_id}: {e:?}");
                    }
                }
            }
            UnifiedCommand::SubmitComment { text, .. } => {
                if let Some(new_comment) = episode.comments.last() {
                    // Persist comment row using the actual stored author/id/timestamp
                    let row = CommentRow {
                        episode_id: episode_id.into(),
                        comment_id: new_comment.id,
                        author: new_comment.author.clone(),
                        text: new_comment.text.clone(),
                        timestamp: new_comment.timestamp,
                    };
                    if let Err(e) = self.store.add_comment(row) {
                        warn!("indexer: failed to add comment for ep {episode_id}: {e:?}");
                    }
                } else if let Some(pk) = authorization {
                    // Fallback: use command data when episode.comments not visible (shouldn't happen)
                    let row = CommentRow {
                        episode_id: episode_id.into(),
                        comment_id: 0,
                        author: format!("{pk}"),
                        text: text.clone(),
                        timestamp: metadata.accepting_time,
                    };
                    if let Err(e) = self.store.add_comment(row) {
                        warn!("indexer: fallback add_comment failed for ep {episode_id}: {e:?}");
                    }
                }
            }
            UnifiedCommand::RequestChallenge | UnifiedCommand::RevokeSession { .. } => {
                // No-op for index storage beyond snapshot refresh
                let snapshot = EpisodeSnapshot {
                    episode_id: episode_id.into(),
                    creator_pubkey: episode.get_creator().map(|p| format!("{p}")),
                    created_at: episode.created_at,
                    authenticated_count: episode.get_authenticated_count(),
                };
                if let Err(e) = self.store.upsert_episode(snapshot) {
                    warn!("indexer: failed to refresh episode {episode_id}: {e:?}");
                }
            }
        }
    }

    fn on_rollback(&self, _episode_id: EpisodeId, _episode: &AuthWithCommentsEpisode) {
        // Simplest approach: ignore for now (store has no deletion API).
        // Future: add tombstone or compaction on reorgs.
    }
}

// Helper reducers (kept for potential external callers/tests)
pub fn apply_new_episode(store: &Store, episode_id: u64, creator_pubkey: Option<String>, created_at: u64) -> Result<(), StoreError> {
    let snapshot = EpisodeSnapshot { episode_id, creator_pubkey, created_at, authenticated_count: 0 };
    store.upsert_episode(snapshot)
}

pub fn apply_auth_update(store: &Store, episode_id: u64, pubkey: &str, _authenticated: bool, _ts: u64) -> Result<(), StoreError> {
    store.add_membership(pubkey, episode_id)
}

pub fn apply_new_comment(
    store: &Store,
    episode_id: u64,
    comment_id: u64,
    author: String,
    text: String,
    timestamp: u64,
) -> Result<(), StoreError> {
    let row = CommentRow { episode_id, comment_id, author, text, timestamp };
    store.add_comment(row)
}
