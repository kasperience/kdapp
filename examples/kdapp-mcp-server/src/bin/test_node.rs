// Test node connection functionality
use anyhow::Result;
use kdapp_mcp_server::node_connector::{connect_to_node, NodeConfig};

#[tokio::main]
async fn main() -> Result<()> {
    println!("🧪 Testing node connection functionality...");

    // Test node configuration
    let node_config = NodeConfig::default();

    println!("🔧 Node configuration:");
    println!("   Network: {:?}", node_config.network_id);
    if let Some(ref url) = node_config.rpc_url {
        println!("   RPC URL: {url}");
    } else {
        println!("   RPC URL: Using default resolver");
    }

    // Test connecting to node
    match connect_to_node(node_config).await {
        Ok(_client) => {
            println!("✅ Node connection test completed successfully!");
        }
        Err(e) => {
            println!("⚠️  Node connection test failed: {e}");
            println!("   This is expected if no local node is running");
        }
    }

    println!("🎉 Node connection test finished!");

    Ok(())
}
