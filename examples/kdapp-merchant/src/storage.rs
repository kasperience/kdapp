use once_cell::sync::Lazy;
use sled::Db;
use std::collections::BTreeMap;
use std::env;

use super::episode::{CustomerInfo, Invoice, Subscription};
use kdapp::pki::PubKey;
use secp256k1::PublicKey as SecpPublicKey;

// Allows running multiple merchant processes concurrently by overriding the DB path.
// Set MERCHANT_DB_PATH to a unique directory per process (e.g., merchant-udp.db, merchant-tcp.db).
pub static DB: Lazy<Db> = Lazy::new(|| {
    let path = env::var("MERCHANT_DB_PATH").unwrap_or_else(|_| "merchant.db".to_string());
    sled::open(&path).unwrap_or_else(|e| panic!("failed to open {path}: {e}"))
});

pub fn init() {
    Lazy::force(&DB);
    let _invoices = DB.open_tree("invoices").expect("invoices tree");
    let _customers = DB.open_tree("customers").expect("customers tree");
    let _subscriptions = DB.open_tree("subscriptions").expect("subscriptions tree");
    #[cfg(test)]
    {
        // Ensure clean state for each test run
        let _ = _invoices.clear();
        let _ = _customers.clear();
        let _ = _subscriptions.clear();
    }
}

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
    let key = sub.id.to_be_bytes();
    let val = borsh::to_vec(sub).expect("serialize subscription");
    let _ = tree.insert(key, val);
}

pub fn delete_subscription(id: u64) {
    let tree = DB.open_tree("subscriptions").expect("subscriptions tree");
    let _ = tree.remove(id.to_be_bytes());
}
