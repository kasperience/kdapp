// Test wallet functionality
use anyhow::Result;
use kdapp_mcp_server::AgentWallet;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🧪 Testing wallet functionality...");
    
    // Test loading/creating agent wallets
    let agent1_wallet = AgentWallet::load_or_create_for_agent("test-agent1")?;
    let agent2_wallet = AgentWallet::load_or_create_for_agent("test-agent2")?;
    
    println!("✅ Agent 1 wallet loaded:");
    println!("   Address: {}", agent1_wallet.get_kaspa_address());
    println!("   Public Key: {}", agent1_wallet.get_public_key_hex());
    
    println!("✅ Agent 2 wallet loaded:");
    println!("   Address: {}", agent2_wallet.get_kaspa_address());
    println!("   Public Key: {}", agent2_wallet.get_public_key_hex());
    
    println!("🎉 Wallet test completed successfully!");
    
    Ok(())
}