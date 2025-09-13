use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Invoice = Value;
pub type Mempool = Value;
pub type GuardianMetrics = Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Webhook {
    pub url: String,
}

pub fn invoice_to_string(inv: &Invoice) -> String {
    if let Some(obj) = inv.as_object() {
        if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
            return id.to_string();
        }
    }
    inv.to_string()
}
