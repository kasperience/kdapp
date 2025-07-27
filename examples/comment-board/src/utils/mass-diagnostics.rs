/// Add to src/utils/mod.rs
pub mod diagnostics {
    use kaspa_consensus_core::tx::Transaction;
    
    /// Estimate transaction mass (reverse-engineered from error messages)
    pub fn estimate_transaction_mass(tx: &Transaction, input_amounts: &[u64]) -> u64 {
        // Base mass from transaction size
        let tx_size = estimate_tx_size(tx);
        let base_mass = tx_size * 1000; // Mass unit per byte
        
        // Input mass (this seems to be the issue)
        let input_mass: u64 = input_amounts.iter()
            .map(|&amount| {
                // Hypothesis: mass includes input amount to prevent dust attacks
                // The error suggests mass ≈ input_amount / 1000
                amount / 1000
            })
            .sum();
        
        // Script complexity mass
        let script_mass = tx.inputs.len() as u64 * 1000;
        
        let total_mass = base_mass + input_mass + script_mass;
        
        println!("🔍 Transaction Mass Breakdown:");
        println!("  Base mass (size): {}", base_mass);
        println!("  Input mass: {}", input_mass);
        println!("  Script mass: {}", script_mass);
        println!("  Total mass: {}", total_mass);
        println!("  Max allowed: 100,000");
        
        total_mass
    }
    
    fn estimate_tx_size(tx: &Transaction) -> u64 {
        // Rough estimation of transaction size in bytes
        let input_size = tx.inputs.len() * 180; // ~180 bytes per input
        let output_size = tx.outputs.len() * 50; // ~50 bytes per output
        let overhead = 100; // Headers, version, etc.
        let payload_size = tx.payload.len();
        
        (input_size + output_size + overhead + payload_size) as u64
    }
}

/// Use in participant/mod.rs before submitting:
use crate::utils::diagnostics::estimate_transaction_mass;

// Before submitting bond transaction
let estimated_mass = estimate_transaction_mass(&bond_tx, &[source_entry.amount]);
if estimated_mass > 100_000 {
    warn!("⚠️ Transaction mass {} exceeds limit! Need to use smaller UTXO", estimated_mass);
    // Use UTXO splitting strategy
}