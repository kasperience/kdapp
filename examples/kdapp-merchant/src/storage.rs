use std::collections::BTreeMap;
use once_cell::sync::Lazy;
use sled::Db;

use crate::episode::Invoice;

pub static DB: Lazy<Db> = Lazy::new(|| {
    sled::open("merchant.db").expect("failed to open merchant.db")
});

pub fn init() {
    Lazy::force(&DB);
    let _ = DB.open_tree("invoices");
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

pub fn flush() {
    let _ = DB.flush();
}
