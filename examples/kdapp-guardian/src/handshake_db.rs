use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::pki::PubKey;
use thiserror::Error;

pub const HANDSHAKE_SCHEMA_VERSION: u32 = 1;
const META_TREE: &str = "__meta";
const HANDSHAKES_TREE: &str = "handshakes";
const SCHEMA_KEY: &[u8] = b"schema_version";

#[derive(Debug, Error)]
pub enum HandshakeStoreError {
    #[error("I/O error while preparing handshake storage: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] sled::Error),
    #[error("unsupported handshake schema version {found}")]
    UnsupportedVersion { found: u32 },
    #[error("invalid handshake schema marker")]
    InvalidSchemaMarker,
    #[error("failed to serialize handshake record")]
    Serialize,
    #[error("failed to deserialize handshake record")]
    Deserialize,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HandshakeRecord {
    pub merchant: PubKey,
    pub guardian: PubKey,
    pub last_seen: u64,
}

pub struct HandshakeStore {
    db: sled::Db,
    tree: sled::Tree,
}

impl HandshakeStore {
    pub fn open(path: &Path) -> Result<Self, HandshakeStoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let db = sled::Config::new().path(path).open()?;
        ensure_schema(&db)?;
        let tree = db.open_tree(HANDSHAKES_TREE)?;
        Ok(Self { db, tree })
    }

    pub fn record_handshake(
        &self,
        merchant: PubKey,
        guardian: PubKey,
    ) -> Result<(), HandshakeStoreError> {
        let record = HandshakeRecord { merchant, guardian, last_seen: current_timestamp() };
        let key = merchant.0.serialize();
        let bytes = borsh::to_vec(&record).map_err(|_| HandshakeStoreError::Serialize)?;
        self.tree.insert(key, bytes)?;
        Ok(())
    }

    pub fn load_all(&self) -> Result<Vec<HandshakeRecord>, HandshakeStoreError> {
        let mut records = Vec::new();
        for entry in self.tree.iter() {
            let (_key, value) = entry?;
            let record = HandshakeRecord::try_from_slice(&value).map_err(|_| HandshakeStoreError::Deserialize)?;
            records.push(record);
        }
        Ok(records)
    }

    pub fn schema_version(&self) -> Result<u32, HandshakeStoreError> {
        let meta = self.db.open_tree(META_TREE)?;
        let Some(raw) = meta.get(SCHEMA_KEY)? else {
            return Err(HandshakeStoreError::InvalidSchemaMarker);
        };
        let arr: [u8; 4] = raw.as_ref().try_into().map_err(|_| HandshakeStoreError::InvalidSchemaMarker)?;
        Ok(u32::from_le_bytes(arr))
    }

    pub fn flush(&self) -> Result<(), HandshakeStoreError> {
        self.db.flush()?;
        Ok(())
    }
}

fn ensure_schema(db: &sled::Db) -> Result<(), HandshakeStoreError> {
    let meta = db.open_tree(META_TREE)?;
    let stored = meta.get(SCHEMA_KEY)?;
    match stored {
        Some(raw) => {
            let arr: [u8; 4] = raw.as_ref().try_into().map_err(|_| HandshakeStoreError::InvalidSchemaMarker)?;
            let version = u32::from_le_bytes(arr);
            if version > HANDSHAKE_SCHEMA_VERSION {
                return Err(HandshakeStoreError::UnsupportedVersion { found: version });
            }
            if version < HANDSHAKE_SCHEMA_VERSION {
                run_migrations(db, version)?;
                meta.insert(SCHEMA_KEY, HANDSHAKE_SCHEMA_VERSION.to_le_bytes())?;
                db.flush()?;
            }
        }
        None => {
            run_migrations(db, 0)?;
            meta.insert(SCHEMA_KEY, HANDSHAKE_SCHEMA_VERSION.to_le_bytes())?;
            db.flush()?;
        }
    }
    Ok(())
}

fn run_migrations(db: &sled::Db, from_version: u32) -> Result<(), HandshakeStoreError> {
    match from_version {
        0 => {
            db.open_tree(HANDSHAKES_TREE)?;
            Ok(())
        }
        v if v == HANDSHAKE_SCHEMA_VERSION => Ok(()),
        v => Err(HandshakeStoreError::UnsupportedVersion { found: v }),
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdapp::pki::generate_keypair;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    fn temp_db_path(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let idx = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        std::env::temp_dir().join(format!(
            "{label}_{}_{}",
            std::process::id(),
            idx
        ))
    }

    #[test]
    fn initializes_schema_for_fresh_database() {
        let path = temp_db_path("guardian_handshake_fresh");
        let store = HandshakeStore::open(&path).expect("open store");
        assert_eq!(store.schema_version().unwrap(), HANDSHAKE_SCHEMA_VERSION);
        drop(store);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn upgrades_legacy_database_without_schema_marker() {
        let path = temp_db_path("guardian_handshake_legacy");
        let (_sk_m, pk_m) = generate_keypair();
        let (_sk_g, pk_g) = generate_keypair();
        {
            let db = sled::Config::new().path(&path).open().expect("legacy db");
            let tree = db.open_tree(HANDSHAKES_TREE).expect("legacy tree");
            let record = HandshakeRecord { merchant: pk_m, guardian: pk_g, last_seen: 42 };
            tree
                .insert(pk_m.0.serialize(), borsh::to_vec(&record).expect("serialize legacy"))
                .expect("insert legacy");
            db.flush().expect("flush legacy");
        }

        let store = HandshakeStore::open(&path).expect("upgrade store");
        assert_eq!(store.schema_version().unwrap(), HANDSHAKE_SCHEMA_VERSION);
        let records = store.load_all().expect("load records");
        assert_eq!(records, vec![HandshakeRecord { merchant: pk_m, guardian: pk_g, last_seen: 42 }]);
        drop(store);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn rejects_future_schema_version() {
        let path = temp_db_path("guardian_handshake_future");
        {
            let db = sled::Config::new().path(&path).open().expect("future db");
            let meta = db.open_tree(META_TREE).expect("meta tree");
            meta
                .insert(
                    SCHEMA_KEY,
                    (HANDSHAKE_SCHEMA_VERSION + 1).to_le_bytes(),
                )
                .expect("insert meta");
            db.flush().expect("flush future");
        }

        let err = HandshakeStore::open(&path).unwrap_err();
        assert!(matches!(err, HandshakeStoreError::UnsupportedVersion { .. }));
        let _ = std::fs::remove_dir_all(path);
    }
}
