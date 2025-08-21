mod app;
mod jsonrpc;
mod node_connector;
mod rpc_handlers;
mod state;
mod tools;
mod wallet;
use anyhow::Result;
use jsonrpc::{Request, Response};
use node_connector::{connect_to_node, NodeConfig};
use state::ServerState;
use wallet::AgentWallet;

#[tokio::main]
async fn main() -> Result<()> {
    app::run().await
}
