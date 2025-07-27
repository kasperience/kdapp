/// Add to wallet/utxo_manager.rs
impl UtxoLockManager {
    /// Create minimal bond transaction to avoid mass limit
    async fn create_minimal_bond_transaction(
        &self,
        comment_id: u64,
        bond_amount: u64,
        source_outpoint: &TransactionOutpoint,
        source_entry: &UtxoEntry,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutput};
        use kaspa_consensus_core::subnets::SubnetworkId;
        use kaspa_txscript::pay_to_address_script;
        use kaspa_consensus_core::sign::sign;
        use kaspa_consensus_core::tx::MutableTransaction;
        
        info!("📡 Creating MINIMAL bond transaction to avoid mass limit...");
        
        // Create minimal payload - no kdapp pattern matching
        let payload = format!("B{}:{}", comment_id, bond_amount).into_bytes();
        
        // Calculate exact change
        let fee = 5000; // Minimal fee
        let change = source_entry.amount - fee;
        
        // Create single input
        let input = TransactionInput {
            previous_outpoint: source_outpoint.clone(),
            signature_script: vec![],
            sequence: 0,
            sig_op_count: 1,
        };
        
        // Create single output (change back to self)
        let script_pubkey = pay_to_address_script(&self.kaspa_address);
        let output = TransactionOutput {
            value: change,
            script_public_key: script_pubkey,
        };
        
        // Build minimal transaction
        let unsigned_tx = Transaction::new(
            1, // version
            vec![input],
            vec![output],
            0, // lock_time
            SubnetworkId::from_bytes([0; 20]),
            0, // gas
            payload,
        );
        
        // Sign transaction
        let signed_tx = sign(
            MutableTransaction::with_entries(unsigned_tx, vec![source_entry.clone()]),
            self.keypair,
        );
        
        let tx_id = signed_tx.id().to_string();
        
        // Submit transaction
        match self.kaspad_client.submit_transaction((&signed_tx).into(), false).await {
            Ok(_) => {
                info!("✅ Minimal bond transaction {} submitted successfully", tx_id);
                Ok(tx_id)
            }
            Err(e) => {
                error!("❌ Failed to submit minimal bond transaction: {}", e);
                Err(format!("Transaction submission failed: {}", e).into())
            }
        }
    }
    
    /// Updated lock_utxo_for_comment to use minimal transaction
    pub async fn lock_utxo_for_comment(
        &mut self,
        comment_id: u64,
        bond_amount: u64,
        lock_duration_seconds: u64,
    ) -> Result<String, String> {
        // ... existing validation code ...
        
        // Find smallest UTXO that can cover the fee
        let min_utxo = self.available_utxos.iter()
            .filter(|(_, entry)| entry.amount >= 10000) // At least 0.0001 KAS for fee
            .min_by_key(|(_, entry)| entry.amount)
            .cloned();
            
        if let Some((outpoint, entry)) = min_utxo {
            // Use minimal transaction instead of kdapp generator
            match self.create_minimal_bond_transaction(comment_id, bond_amount, &outpoint, &entry).await {
                Ok(bond_tx_id) => {
                    // ... rest of the tracking code ...
                    Ok(bond_tx_id)
                }
                Err(e) => Err(format!("Failed to create bond transaction: {}", e))
            }
        } else {
            Err("No suitable UTXO for bond transaction".to_string())
        }
    }
}