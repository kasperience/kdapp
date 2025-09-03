mod app;
mod jsonrpc;
mod node_connector;
mod rpc_handlers;
mod state;
mod tools;
mod wallet;
mod routing;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    app::run().await
}
