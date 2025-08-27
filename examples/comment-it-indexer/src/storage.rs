use crate::models::{CommentRow, EpisodeDetail, EpisodeSnapshot};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error("internal error")] Internal,
}

pub type Store = Arc<dyn StoreTrait + Send + Sync + 'static>;

pub trait StoreTrait {
    fn upsert_episode(&self, ep: EpisodeSnapshot) -> Result<(), StoreError>;
    fn add_comment(&self, row: CommentRow) -> Result<(), StoreError>;
    fn add_membership(&self, pubkey: &str, episode_id: u64) -> Result<(), StoreError>;

    fn get_episode(&self, id: u64, recent: usize) -> Result<Option<EpisodeDetail>, StoreError>;
    fn get_comments_after(&self, id: u64, after_ts: u64, limit: usize) -> Result<Vec<CommentRow>, StoreError>;
    fn get_recent(&self, limit: usize) -> Result<Vec<EpisodeSnapshot>, StoreError>;
    fn get_my_episodes(&self, pubkey: &str, limit: usize) -> Result<Vec<u64>, StoreError>;
}

#[cfg(feature = "mem-store")]
#[derive(Default)]
struct Mem {
    episodes: Mutex<HashMap<u64, EpisodeSnapshot>>,               // id -> snapshot
    comments: Mutex<BTreeMap<(u64, u64), CommentRow>>,            // (episode_id, comment_id) -> row
    memberships: Mutex<HashMap<String, BTreeSet<u64>>>,           // pubkey -> set of episode ids
    recent_order: Mutex<BTreeMap<u64, u64>>,                      // created_at -> episode_id
}

#[cfg(feature = "mem-store")]
impl StoreTrait for Mem {
    fn upsert_episode(&self, ep: EpisodeSnapshot) -> Result<(), StoreError> {
        self.episodes.lock().map_err(|_| StoreError::Internal)?.insert(ep.episode_id, ep.clone());
        self.recent_order.lock().map_err(|_| StoreError::Internal)?.insert(ep.created_at, ep.episode_id);
        Ok(())
    }

    fn add_comment(&self, row: CommentRow) -> Result<(), StoreError> {
        self.comments.lock().map_err(|_| StoreError::Internal)?.insert((row.episode_id, row.comment_id), row);
        Ok(())
    }

    fn add_membership(&self, pubkey: &str, episode_id: u64) -> Result<(), StoreError> {
        let mut m = self.memberships.lock().map_err(|_| StoreError::Internal)?;
        m.entry(pubkey.to_string()).or_default().insert(episode_id);
        Ok(())
    }

    fn get_episode(&self, id: u64, recent: usize) -> Result<Option<EpisodeDetail>, StoreError> {
        let ep = match self.episodes.lock().map_err(|_| StoreError::Internal)?.get(&id).cloned() {
            Some(v) => v,
            None => return Ok(None),
        };
        let mut rows = Vec::new();
        for ((_eid, _cid), row) in self.comments.lock().map_err(|_| StoreError::Internal)?.iter().rev() {
            if *_eid == id {
                rows.push(row.clone());
                if rows.len() >= recent { break; }
            }
        }
        Ok(Some(EpisodeDetail { snapshot: ep, recent_comments: rows }))
    }

    fn get_comments_after(&self, id: u64, after_ts: u64, limit: usize) -> Result<Vec<CommentRow>, StoreError> {
        let mut out = Vec::new();
        for ((_eid, _cid), row) in self.comments.lock().map_err(|_| StoreError::Internal)?.iter() {
            if *_eid == id && row.timestamp > after_ts {
                out.push(row.clone());
                if out.len() >= limit { break; }
            }
        }
        Ok(out)
    }

    fn get_recent(&self, limit: usize) -> Result<Vec<EpisodeSnapshot>, StoreError> {
        let order = self.recent_order.lock().map_err(|_| StoreError::Internal)?;
        let episodes = self.episodes.lock().map_err(|_| StoreError::Internal)?;
        let mut out = Vec::new();
        for (_ts, id) in order.iter().rev() {
            if let Some(ep) = episodes.get(id) { out.push(ep.clone()); }
            if out.len() >= limit { break; }
        }
        Ok(out)
    }

    fn get_my_episodes(&self, pubkey: &str, limit: usize) -> Result<Vec<u64>, StoreError> {
        let set = match self.memberships.lock().map_err(|_| StoreError::Internal)?.get(pubkey) {
            Some(s) => s.clone(),
            None => return Ok(vec![]),
        };
        Ok(set.iter().copied().take(limit).collect())
    }
}

pub fn new_store() -> Result<Store, StoreError> {
    #[cfg(feature = "mem-store")]
    {
        return Ok(Arc::new(Mem::default()))
    }
    #[allow(unreachable_code)]
    Err(StoreError::Internal)
}
