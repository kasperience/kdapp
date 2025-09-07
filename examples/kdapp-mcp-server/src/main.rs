mod app;
mod jsonrpc;
mod node_connector;
mod routing;
mod rpc_handlers;
mod state;
mod tools;
mod wallet;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    app::run().await
}
