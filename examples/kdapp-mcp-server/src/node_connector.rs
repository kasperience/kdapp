#![allow(dead_code)]
// src/node_connector.rs - Node connection functionality for kdapp MCP Server
use anyhow::Result;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_wrpc_client::{prelude::RpcApi, KaspaRpcClient};
use kdapp::proxy::connect_client;
use std::env;

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub network_id: NetworkId,
    pub rpc_url: Option<String>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        // Support custom RPC URL via environment variable
        let rpc_url = env::var("KASPA_RPC_URL").ok();

        // Network selection via KDAPP_NETWORK env: e.g., "mainnet", "testnet-10"
        let network_id = match env::var("KDAPP_NETWORK").ok().as_deref() {
            Some("mainnet") => NetworkId::with_suffix(NetworkType::Mainnet, 0),
            Some(val) if val.starts_with("testnet-") => {
                let suffix = val.trim_start_matches("testnet-").parse::<u32>().unwrap_or(10);
                NetworkId::with_suffix(NetworkType::Testnet, suffix)
            }
            _ => NetworkId::with_suffix(NetworkType::Testnet, 10),
        };

        Self { network_id, rpc_url }
    }
}

/// Connect to a Kaspa node
pub async fn connect_to_node(config: NodeConfig) -> Result<KaspaRpcClient> {
    println!("📡 Connecting to Kaspa network: {:?}", config.network_id);

    if let Some(ref url) = config.rpc_url {
        println!("🔗 Using custom RPC URL: {url}");
    }

    let client = connect_client(config.network_id, config.rpc_url).await?;

    println!("✅ Successfully connected to Kaspa node");

    // Get server info
    let server_info = client.get_server_info().await?;
    println!("ℹ️  Server version: {}", server_info.server_version);
    println!("ℹ️  Network: {:?}", server_info.network_id);
    println!("ℹ️  Sync status: {}", if server_info.is_synced { "SYNCED" } else { "NOT SYNCED" });

    Ok(client)
}

/// Check if the node connection is healthy
pub async fn check_node_health(client: &KaspaRpcClient) -> Result<bool> {
    let server_info = client.get_server_info().await?;
    Ok(server_info.is_synced)
}

/// Get the current network info
pub async fn get_network_info(client: &KaspaRpcClient) -> Result<String> {
    let dag_info = client.get_block_dag_info().await?;
    Ok(format!("DAG Info - Sink: {}, Virtual DAA Score: {}", dag_info.sink, dag_info.virtual_daa_score))
}
