use borsh::{BorshDeserialize, BorshSerialize};
use once_cell::sync::Lazy;
use sled::Db;
use std::collections::BTreeMap;
#[cfg(not(test))]
use std::env;
use std::sync::Once;
use std::thread;

use std::time::Duration;

use super::episode::{CustomerInfo, Invoice, Subscription};
use kdapp::pki::PubKey;
use secp256k1::PublicKey as SecpPublicKey;
use sha2::{Digest, Sha256};
use thiserror::Error;

const SCRIPT_TEMPLATES_TREE: &str = "script_templates";
const SESSION_TOKENS_TREE: &str = "session_tokens";

pub const SCRIPT_TEMPLATE_WHITELIST: &[&str] = &["merchant_p2pk", "merchant_guardian_multisig", "merchant_taproot"];

// Allows running multiple merchant processes concurrently by overriding the DB path.
// Set MERCHANT_DB_PATH to a unique directory per process (e.g., merchant-udp.db, merchant-tcp.db).
pub static DB: Lazy<Db> = Lazy::new(|| {
    #[cfg(test)]
    {
        // Use a temporary, in-memory database during tests to avoid filesystem issues
        sled::Config::new().temporary(true).open().expect("open temp sled")
    }
    #[cfg(not(test))]
    {
        let path = env::var("MERCHANT_DB_PATH").unwrap_or_else(|_| "merchant.db".to_string());
        sled::Config::new().path(&path).flush_every_ms(Some(500)).open().unwrap_or_else(|e| panic!("failed to open {path}: {e}"))
    }
});

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ScriptTemplate {
    pub template_id: String,
    pub script_bytes: Vec<u8>,
    pub description: Option<String>,
}

#[derive(Debug, Error)]
pub enum TemplateStoreError {
    #[error("template identifier cannot be empty")]
    InvalidIdentifier,
    #[error("script template `{0}` is not permitted")]
    NotAllowed(String),
    #[error("script template must include script bytes")]
    EmptyScript,
    #[error("failed to serialize script template")]
    Serialize,
    #[error("database error: {0}")]
    Db(#[from] sled::Error),
}

pub fn init() {
    Lazy::force(&DB);
    Lazy::force(&FLUSH_WORKER);
    let _invoices = DB.open_tree("invoices").expect("invoices tree");
    let _customers = DB.open_tree("customers").expect("customers tree");
    let _subscriptions = DB.open_tree("subscriptions").expect("subscriptions tree");
    let _templates = DB.open_tree(SCRIPT_TEMPLATES_TREE).expect("script templates tree");
    let _sessions = DB.open_tree(SESSION_TOKENS_TREE).expect("session tokens tree");
    #[cfg(test)]
    {
        // Ensure clean state for each test run
        let _ = _invoices.clear();
        let _ = _customers.clear();
        let _ = _subscriptions.clear();
        let _ = _templates.clear();
        let _ = _sessions.clear();
    }
}

static FLUSH_WORKER: Lazy<()> = Lazy::new(|| {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let db = DB.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(60));
            let _ = db.flush();
        });
        let db2 = DB.clone();
        let _ = ctrlc::set_handler(move || {
            let _ = db2.flush();
        });
    });
});

#[cfg_attr(test, allow(dead_code))]
static COMPACT_ONCE: Once = Once::new();

#[cfg_attr(test, allow(dead_code))]
pub fn start_compaction(interval_ms: u64) {
    COMPACT_ONCE.call_once(|| {
        let db = DB.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(interval_ms));
            if let Err(e) = db.flush() {
                log::error!("Flush failed: {e}");
            }
        });
    });
}

#[cfg_attr(test, allow(dead_code))]
pub fn load_invoices() -> BTreeMap<u64, Invoice> {
    let tree = DB.open_tree("invoices").expect("invoices tree");
    tree.iter()
        .filter_map(|res| res.ok())
        .filter_map(|(k, v)| {
            if k.len() == 8 {
                let mut id_bytes = [0u8; 8];
                id_bytes.copy_from_slice(&k);
                let id = u64::from_be_bytes(id_bytes);
                borsh::from_slice::<Invoice>(&v).ok().map(|inv| (id, inv))
            } else {
                None
            }
        })
        .collect()
}

pub fn put_invoice(inv: &Invoice) {
    let tree = DB.open_tree("invoices").expect("invoices tree");
    let key = inv.id.to_be_bytes();
    let val = borsh::to_vec(inv).expect("serialize invoice");
    let _ = tree.insert(key, val);
}

pub fn delete_invoice(id: u64) {
    let tree = DB.open_tree("invoices").expect("invoices tree");
    let _ = tree.remove(id.to_be_bytes());
}

#[allow(dead_code)]
pub fn flush() {
    let _ = DB.flush();
}

#[cfg_attr(test, allow(dead_code))]
pub fn load_customers() -> BTreeMap<PubKey, CustomerInfo> {
    let tree = DB.open_tree("customers").expect("customers tree");
    tree.iter()
        .filter_map(|res| res.ok())
        .filter_map(|(k, v)| {
            if k.len() == 33 {
                let mut pk_bytes = [0u8; 33];
                pk_bytes.copy_from_slice(&k);
                SecpPublicKey::from_slice(&pk_bytes)
                    .ok()
                    .map(PubKey)
                    .and_then(|pk| borsh::from_slice::<CustomerInfo>(&v).ok().map(|info| (pk, info)))
            } else {
                None
            }
        })
        .collect()
}

pub fn put_customer(pk: &PubKey, info: &CustomerInfo) {
    let tree = DB.open_tree("customers").expect("customers tree");
    let key = pk.0.serialize();
    let val = borsh::to_vec(info).expect("serialize customer");
    let _ = tree.insert(key, val);
}

#[cfg_attr(test, allow(dead_code))]
pub fn load_subscriptions() -> BTreeMap<u64, Subscription> {
    let tree = DB.open_tree("subscriptions").expect("subscriptions tree");
    tree.iter()
        .filter_map(|res| res.ok())
        .filter_map(|(k, v)| {
            if k.len() == 8 {
                let mut id_bytes = [0u8; 8];
                id_bytes.copy_from_slice(&k);
                let id = u64::from_be_bytes(id_bytes);
                borsh::from_slice::<Subscription>(&v).ok().map(|sub| (id, sub))
            } else {
                None
            }
        })
        .collect()
}

pub fn put_subscription(sub: &Subscription) {
    let tree = DB.open_tree("subscriptions").expect("subscriptions tree");
    let key = sub.sub_id.to_be_bytes();
    let val = borsh::to_vec(sub).expect("serialize subscription");
    let _ = tree.insert(key, val);
}

pub fn delete_subscription(id: u64) {
    let tree = DB.open_tree("subscriptions").expect("subscriptions tree");
    let _ = tree.remove(id.to_be_bytes());
}

fn canonical_template_id(id: &str) -> String {
    id.trim().to_ascii_lowercase()
}

fn template_allowed(id: &str) -> bool {
    SCRIPT_TEMPLATE_WHITELIST.iter().any(|allowed| allowed.eq_ignore_ascii_case(id))
}

pub fn put_script_template(template: &ScriptTemplate) -> Result<(), TemplateStoreError> {
    let mut normalized = template.clone();
    let identifier = canonical_template_id(&normalized.template_id);
    if identifier.is_empty() {
        return Err(TemplateStoreError::InvalidIdentifier);
    }
    if !template_allowed(&identifier) {
        return Err(TemplateStoreError::NotAllowed(identifier));
    }
    if normalized.script_bytes.is_empty() {
        return Err(TemplateStoreError::EmptyScript);
    }
    normalized.template_id = identifier.clone();
    let tree = DB.open_tree(SCRIPT_TEMPLATES_TREE)?;
    let value = borsh::to_vec(&normalized).map_err(|_| TemplateStoreError::Serialize)?;
    tree.insert(identifier.as_bytes(), value)?;
    Ok(())
}

pub fn load_script_templates() -> BTreeMap<String, ScriptTemplate> {
    let tree = DB.open_tree(SCRIPT_TEMPLATES_TREE).expect("script templates tree");
    tree.iter()
        .filter_map(|res| res.ok())
        .filter_map(|(_, v)| borsh::from_slice::<ScriptTemplate>(&v).ok())
        .map(|template| {
            let id = template.template_id.clone();
            (id, template)
        })
        .collect()
}

pub fn delete_script_template(template_id: &str) -> Result<(), sled::Error> {
    let id = canonical_template_id(template_id);
    if id.is_empty() {
        return Ok(());
    }
    let tree = DB.open_tree(SCRIPT_TEMPLATES_TREE)?;
    let _ = tree.remove(id.as_bytes())?;
    Ok(())
}

fn session_token_key(token: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[allow(dead_code)]
pub fn store_session_token(token: &str) -> Result<(), sled::Error> {
    if token.is_empty() {
        return Ok(());
    }
    let tree = DB.open_tree(SESSION_TOKENS_TREE)?;
    let key = session_token_key(token);
    tree.insert(key, &[])?;
    Ok(())
}

#[allow(dead_code)]
pub fn remove_session_token(token: &str) -> Result<(), sled::Error> {
    if token.is_empty() {
        return Ok(());
    }
    let tree = DB.open_tree(SESSION_TOKENS_TREE)?;
    let key = session_token_key(token);
    let _ = tree.remove(key)?;
    Ok(())
}

pub fn session_token_exists(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let tree = DB.open_tree(SESSION_TOKENS_TREE).expect("session tokens tree");
    tree.contains_key(session_token_key(token)).unwrap_or(false)
}
