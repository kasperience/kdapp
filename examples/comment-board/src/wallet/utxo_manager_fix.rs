/// Add this method to UtxoLockManager in wallet/utxo_manager.rs
impl UtxoLockManager {
    /// Split a large UTXO into smaller ones to avoid mass limit issues
    pub async fn split_large_utxo(
        &mut self,
        max_utxo_size: u64, // e.g., 10 KAS = 1_000_000_000 sompi
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::utils::{PATTERN, PREFIX, FEE};
        use kdapp::generator::TransactionGenerator;
        
        // Find UTXOs larger than max_utxo_size
        let large_utxos: Vec<_> = self.available_utxos.iter()
            .filter(|(_, entry)| entry.amount > max_utxo_size)
            .cloned()
            .collect();
            
        if large_utxos.is_empty() {
            info!("No large UTXOs to split");
            return Ok(());
        }
        
        for (outpoint, entry) in large_utxos {
            let total_amount = entry.amount;
            let num_splits = (total_amount / max_utxo_size) + 1;
            let amount_per_split = total_amount / num_splits;
            
            info!("Splitting UTXO of {:.6} KAS into {} parts of {:.6} KAS each",
                  total_amount as f64 / 100_000_000.0,
                  num_splits,
                  amount_per_split as f64 / 100_000_000.0);
            
            // Create splitting transaction
            let generator = TransactionGenerator::new(self.keypair, PATTERN, PREFIX);
            
            // Build transaction with multiple outputs
            let split_tx = generator.build_transaction(
                &[(outpoint.clone(), entry.clone())],
                amount_per_split,
                num_splits,
                &self.kaspa_address,
                b"SPLIT_UTXO".to_vec(),
            );
            
            // Submit split transaction
            match self.kaspad_client.submit_transaction((&split_tx).into(), false).await {
                Ok(_) => {
                    info!("✅ Split transaction {} submitted successfully", split_tx.id());
                    // Wait a bit for confirmation
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    
                    // Refresh UTXOs to get the new smaller ones
                    self.refresh_utxos(&self.kaspad_client).await?;
                }
                Err(e) => {
                    error!("❌ Failed to submit split transaction: {}", e);
                    return Err(format!("Split transaction failed: {}", e).into());
                }
            }
        }
        
        Ok(())
    }
}

/// In participant/mod.rs, before creating bonds:
async fn run_comment_board(...) {
    // ... existing code ...
    
    // Split large UTXOs before starting
    info!("🔄 Checking for large UTXOs that need splitting...");
    if let Err(e) = utxo_manager.split_large_utxo(10_000_000_000).await { // Split anything > 10 KAS
        warn!("Failed to split UTXOs: {}", e);
    }
    
    // ... rest of the code ...
}