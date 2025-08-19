mod jsonrpc;
mod state;
mod tools;
mod wallet;
mod node_connector;
mod app;
mod rpc_handlers;
use anyhow::Result;
use jsonrpc::{Request, Response};
use state::ServerState;
use wallet::AgentWallet;
use node_connector::{connect_to_node, NodeConfig};

#[tokio::main]
async fn main() -> Result<()> { app::run().await }
