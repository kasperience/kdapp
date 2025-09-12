use std::time::Duration;

use hex::encode;
use hmac::{Hmac, Mac};
use log::info;
use reqwest::Client;
use serde::Serialize;
use sha2::Sha256;
use thiserror::Error;

const RETRY_DELAYS: [u64; 3] = [1, 2, 4];

#[derive(Serialize)]
pub struct WebhookEvent {
    pub event: String,
    pub invoice_id: u64,
    pub amount: u64,
    pub timestamp: u64,
}

#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("http status {0}")]
    Http(u16),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error(transparent)]
    Serialize(#[from] serde_json::Error),
    #[error(transparent)]
    InvalidSecret(#[from] hmac::digest::InvalidLength),
}

pub async fn post_event(url: &str, secret: &[u8], event: &WebhookEvent) -> Result<(), WebhookError> {
    let body = serde_json::to_vec(event)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret)?;
    mac.update(&body);
    let signature = encode(mac.finalize().into_bytes());

    let client = Client::builder().timeout(Duration::from_secs(3)).build()?;

    for attempt in 1..=RETRY_DELAYS.len() + 1 {
        let res = client
            .post(url)
            .header("X-Signature", &signature)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.clone())
            .send()
            .await;
        match res {
            Ok(resp) => {
                let status = resp.status();
                info!("webhook event={} invoice_id={} attempt={} status={}", event.event, event.invoice_id, attempt, status.as_u16());
                if status.is_success() {
                    return Ok(());
                }
                if status.is_server_error() && attempt <= RETRY_DELAYS.len() {
                    tokio::time::sleep(Duration::from_secs(RETRY_DELAYS[attempt - 1])).await;
                    continue;
                }
                return Err(WebhookError::Http(status.as_u16()));
            }
            Err(err) => {
                info!("webhook event={} invoice_id={} attempt={} status={}", event.event, event.invoice_id, attempt, err);
                if attempt <= RETRY_DELAYS.len() {
                    tokio::time::sleep(Duration::from_secs(RETRY_DELAYS[attempt - 1])).await;
                    continue;
                }
                return Err(WebhookError::Request(err));
            }
        }
    }

    unreachable!()
}
