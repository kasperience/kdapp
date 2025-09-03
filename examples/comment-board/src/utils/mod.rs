use kaspa_consensus_core::tx::Transaction;
use kaspa_wrpc_client::prelude::*;
use kdapp::generator::{PatternType, PrefixType};
use kdapp::proxy::connect_options;

// TODO: derive pattern from prefix (using prefix as a random seed for composing the pattern)
pub const PATTERN: PatternType = [(7, 0), (32, 1), (45, 0), (99, 1), (113, 0), (126, 1), (189, 0), (200, 1), (211, 0), (250, 1)];
pub const PREFIX: PrefixType = 858598618;
pub const FEE: u64 = 5000;

/// Submit a transaction with lightweight reconnect-and-retry on websocket errors.
pub async fn submit_tx_retry(kaspad: &KaspaRpcClient, tx: &Transaction, attempts: usize) -> Result<(), String> {
    let mut tries = 0usize;
    loop {
        match kaspad.submit_transaction(tx.into(), false).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                tries += 1;
                let msg = e.to_string();
                if tries >= attempts {
                    return Err(format!("submit failed after {tries} attempts: {msg}"));
                }
                // Try reconnect on websocket-related issues, then retry
                if msg.contains("WebSocket") || msg.contains("not connected") || msg.contains("disconnected") {
                    let _ = kaspad.connect(Some(connect_options())).await;
                    continue;
                } else if msg.contains("orphan") {
                    // Minor transient; just retry once more without reconnect
                    continue;
                } else if msg.contains("already accepted") {
                    // Treat as success from client perspective
                    return Ok(());
                } else {
                    return Err(format!("submit failed: {msg}"));
                }
            }
        }
    }
}
