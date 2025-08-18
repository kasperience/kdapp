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

        let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);

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
    let version = server_info.server_version.clone();
    println!("ℹ️  Server version: {version}");
    println!("ℹ️  Network: {:?}", server_info.network_id);
    let sync_status = if server_info.is_synced { "SYNCED" } else { "NOT SYNCED" };
    println!("ℹ️  Sync status: {sync_status}");

    Ok(client)
}

/// Check if the node connection is healthy
#[allow(dead_code)]
pub async fn check_node_health(client: &KaspaRpcClient) -> Result<bool> {
    let server_info = client.get_server_info().await?;
    Ok(server_info.is_synced)
}

/// Get the current network info
#[allow(dead_code)]
pub async fn get_network_info(client: &KaspaRpcClient) -> Result<String> {
    let dag_info = client.get_block_dag_info().await?;
    let sink = dag_info.sink;
    let daa = dag_info.virtual_daa_score;
    Ok(format!("DAG Info - Sink: {sink}, Virtual DAA Score: {daa}"))
}
