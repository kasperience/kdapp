use crate::models::{CommentRow, EpisodeDetail, EpisodeSnapshot};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error("internal error")]
    Internal,
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

    fn stats(&self) -> Result<StoreStats, StoreError>;
}

#[derive(Default, Clone, Copy)]
pub struct StoreStats {
    pub episodes: usize,
    pub comments: usize,
    pub memberships: usize,
}

#[cfg(feature = "mem-store")]
#[derive(Default)]
struct Mem {
    episodes: Mutex<HashMap<u64, EpisodeSnapshot>>,     // id -> snapshot
    comments: Mutex<BTreeMap<(u64, u64), CommentRow>>,  // (episode_id, comment_id) -> row
    memberships: Mutex<HashMap<String, BTreeSet<u64>>>, // pubkey -> set of episode ids
    recent_order: Mutex<BTreeMap<u64, u64>>,            // created_at -> episode_id
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
                if rows.len() >= recent {
                    break;
                }
            }
        }
        Ok(Some(EpisodeDetail { snapshot: ep, recent_comments: rows }))
    }

    fn get_comments_after(&self, id: u64, after_ts: u64, limit: usize) -> Result<Vec<CommentRow>, StoreError> {
        let mut out = Vec::new();
        for ((_eid, _cid), row) in self.comments.lock().map_err(|_| StoreError::Internal)?.iter() {
            if *_eid == id && row.timestamp > after_ts {
                out.push(row.clone());
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    fn get_recent(&self, limit: usize) -> Result<Vec<EpisodeSnapshot>, StoreError> {
        let order = self.recent_order.lock().map_err(|_| StoreError::Internal)?;
        let episodes = self.episodes.lock().map_err(|_| StoreError::Internal)?;
        let mut out = Vec::new();
        for (_ts, id) in order.iter().rev() {
            if let Some(ep) = episodes.get(id) {
                out.push(ep.clone());
            }
            if out.len() >= limit {
                break;
            }
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

    fn stats(&self) -> Result<StoreStats, StoreError> {
        let episodes = self.episodes.lock().map_err(|_| StoreError::Internal)?.len();
        let comments = self.comments.lock().map_err(|_| StoreError::Internal)?.len();
        let memberships = self.memberships.lock().map_err(|_| StoreError::Internal)?.len();
        Ok(StoreStats { episodes, comments, memberships })
    }
}

pub fn new_store() -> Result<Store, StoreError> {
    #[cfg(all(feature = "mem-store", not(feature = "rocksdb-store")))]
    {
        return Ok(Arc::new(Mem::default()));
    }
    #[cfg(feature = "rocksdb-store")]
    {
        return Ok(Arc::new(Rocks::open_default()?));
    }
    #[allow(unreachable_code)]
    Err(StoreError::Internal)
}

// ================= RocksDB backend =================
#[cfg(feature = "rocksdb-store")]
use rocksdb::{IteratorMode, Options, DB};

#[cfg(feature = "rocksdb-store")]
struct Rocks {
    db: DB,
}

#[cfg(feature = "rocksdb-store")]
impl Rocks {
    fn open_default() -> Result<Self, StoreError> {
        let path = std::env::var("INDEX_DB_PATH").unwrap_or_else(|_| ".kdapp-indexer-db".to_string());
        let mut opts = Options::default();
        opts.create_if_missing(true);
        DB::open(&opts, path).map(|db| Self { db }).map_err(|_| StoreError::Internal)
    }

    fn key_ep(id: u64) -> [u8; 9] {
        let mut k = [0u8; 9];
        k[0] = b'e';
        k[1..].copy_from_slice(&id.to_be_bytes());
        k
    }
    fn key_recent(ts: u64, id: u64) -> [u8; 17] {
        let mut k = [0u8; 17];
        k[0] = b'r';
        k[1..9].copy_from_slice(&ts.to_be_bytes());
        k[9..].copy_from_slice(&id.to_be_bytes());
        k
    }
    fn key_comment_prefix(id: u64) -> [u8; 9] {
        let mut k = [0u8; 9];
        k[0] = b'c';
        k[1..].copy_from_slice(&id.to_be_bytes());
        k
    }
    fn key_comment(id: u64, cid: u64) -> [u8; 17] {
        let mut k = [0u8; 17];
        k[0] = b'c';
        k[1..9].copy_from_slice(&id.to_be_bytes());
        k[9..].copy_from_slice(&cid.to_be_bytes());
        k
    }
    fn key_members(pubkey: &str) -> Vec<u8> {
        let mut v = Vec::with_capacity(1 + pubkey.len());
        v.push(b'm');
        v.extend_from_slice(pubkey.as_bytes());
        v
    }
}

#[cfg(feature = "rocksdb-store")]
impl StoreTrait for Rocks {
    fn upsert_episode(&self, ep: EpisodeSnapshot) -> Result<(), StoreError> {
        let k = Self::key_ep(ep.episode_id);
        let v = bincode::serialize(&ep).map_err(|_| StoreError::Internal)?;
        self.db.put(k, v).map_err(|_| StoreError::Internal)?;
        let r = Self::key_recent(ep.created_at, ep.episode_id);
        self.db.put(r, []).map_err(|_| StoreError::Internal)?;
        Ok(())
    }

    fn add_comment(&self, row: CommentRow) -> Result<(), StoreError> {
        let k = Self::key_comment(row.episode_id, row.comment_id);
        let v = bincode::serialize(&row).map_err(|_| StoreError::Internal)?;
        self.db.put(k, v).map_err(|_| StoreError::Internal)
    }

    fn add_membership(&self, pubkey: &str, episode_id: u64) -> Result<(), StoreError> {
        let k = Self::key_members(pubkey);
        let current = self.db.get(&k).map_err(|_| StoreError::Internal)?;
        let mut list: Vec<u64> = match current {
            Some(bytes) => bincode::deserialize(bytes.as_ref()).unwrap_or_default(),
            None => vec![],
        };
        if !list.contains(&episode_id) {
            list.push(episode_id);
        }
        let v = bincode::serialize(&list).map_err(|_| StoreError::Internal)?;
        self.db.put(k, v).map_err(|_| StoreError::Internal)
    }

    fn get_episode(&self, id: u64, recent: usize) -> Result<Option<EpisodeDetail>, StoreError> {
        let k = Self::key_ep(id);
        let Some(bytes) = self.db.get(k).map_err(|_| StoreError::Internal)? else { return Ok(None) };
        let snapshot: EpisodeSnapshot = bincode::deserialize(bytes.as_ref()).map_err(|_| StoreError::Internal)?;
        let mut rows: Vec<CommentRow> = Vec::new();
        let prefix = Self::key_comment_prefix(id);
        // Collect then reverse for recent tail
        let mut all: Vec<CommentRow> = Vec::new();
        for kv in self.db.prefix_iterator(prefix) {
            if let Ok((_k, v)) = kv {
                if let Ok(r) = bincode::deserialize::<CommentRow>(v.as_ref()) {
                    all.push(r);
                }
            }
        }
        all.sort_by(|a, b| a.comment_id.cmp(&b.comment_id));
        for r in all.into_iter().rev().take(recent) {
            rows.push(r);
        }
        Ok(Some(EpisodeDetail { snapshot, recent_comments: rows }))
    }

    fn get_comments_after(&self, id: u64, after_ts: u64, limit: usize) -> Result<Vec<CommentRow>, StoreError> {
        let mut out = Vec::new();
        let prefix = Self::key_comment_prefix(id);
        for kv in self.db.prefix_iterator(prefix) {
            if let Ok((_k, v)) = kv {
                if let Ok(r) = bincode::deserialize::<CommentRow>(v.as_ref()) {
                    if r.timestamp > after_ts {
                        out.push(r);
                    }
                }
            }
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    fn get_recent(&self, limit: usize) -> Result<Vec<EpisodeSnapshot>, StoreError> {
        let mut out = Vec::new();
        for kv in self.db.iterator(IteratorMode::End) {
            let (k, _v) = kv.map_err(|_| StoreError::Internal)?;
            if !k.is_empty() && k[0] == b'r' && k.len() == 17 {
                let mut idb = [0u8; 8];
                idb.copy_from_slice(&k[9..17]);
                let id = u64::from_be_bytes(idb);
                if let Ok(Some(v)) = self.db.get(Self::key_ep(id)) {
                    if let Ok(ep) = bincode::deserialize::<EpisodeSnapshot>(v.as_ref()) {
                        out.push(ep);
                    }
                }
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    fn get_my_episodes(&self, pubkey: &str, limit: usize) -> Result<Vec<u64>, StoreError> {
        let k = Self::key_members(pubkey);
        let list = match self.db.get(k).map_err(|_| StoreError::Internal)? {
            Some(bytes) => bincode::deserialize(&bytes).unwrap_or_default(),
            None => vec![],
        };
        Ok(list.into_iter().take(limit).collect())
    }

    fn stats(&self) -> Result<StoreStats, StoreError> {
        let mut episodes = 0usize;
        let mut comments = 0usize;
        let mut memberships = 0usize;
        for kv in self.db.iterator(IteratorMode::Start) {
            let (k, _v) = kv.map_err(|_| StoreError::Internal)?;
            if k.first() == Some(&b'e') {
                episodes += 1;
            } else if k.first() == Some(&b'c') {
                comments += 1;
            } else if k.first() == Some(&b'm') {
                memberships += 1;
            }
        }
        Ok(StoreStats { episodes, comments, memberships })
    }
}
