// src/utils/explorer.rs - Explorer link utilities extracted from main.rs

/// Helper function to generate Kaspa explorer links
pub fn print_explorer_links(tx_id: &str, wallet_address: &str) {
    println!("🔗 [ VERIFY ON KASPA EXPLORER → ] https://explorer-tn10.kaspa.org/txs/{tx_id}");
    println!("🔗 [ VIEW WALLET ON EXPLORER → ] https://explorer-tn10.kaspa.org/addresses/{wallet_address}");
}
