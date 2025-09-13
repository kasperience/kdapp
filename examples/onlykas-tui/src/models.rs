use serde::Deserialize;
use serde_json::Value;

pub type Invoice = Value;
pub type Mempool = Value;
pub type GuardianMetrics = Value;

#[derive(Debug, Clone, Deserialize)]
pub struct Subscription {
    pub id: u64,
    #[serde(alias = "amount")]
    pub amount_sompi: u64,
    #[serde(alias = "period_secs")]
    pub interval: u64,
    #[serde(alias = "next_run_ts")]
    pub next_charge_ts: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebhookEvent {
    pub event: String,
    pub id: String,
    pub ts: u64,
    pub details: Value,
}

pub fn invoice_to_string(inv: &Invoice) -> String {
    if let Some(obj) = inv.as_object() {
        if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
            return id.to_string();
        }
    }
    inv.to_string()
}

pub fn subscription_to_string(sub: &Subscription) -> String {
    format!("{} amt {} int {} next {}", sub.id, sub.amount_sompi, sub.interval, sub.next_charge_ts)
}
