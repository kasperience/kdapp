use anyhow::Result;
use serde_json::{from_str, to_string};
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::signal;

use crate::jsonrpc::{Request, Response};
use crate::node_connector::{connect_to_node, NodeConfig};
use crate::state::ServerState;
use crate::wallet::AgentWallet;

pub async fn run() -> Result<()> {
    // Initialize logging (env_logger still used by deps if set)
    env_logger::init();

    // Initialize agent wallets
    println!("🔐 Initializing agent wallets...");
    let agent1_wallet = AgentWallet::load_or_create_for_agent("agent1")?;
    let agent2_wallet = AgentWallet::load_or_create_for_agent("agent2")?;

    agent1_wallet.show_funding_reminder();
    agent2_wallet.show_funding_reminder();

    // Connect to Kaspa node
    println!("📡 Connecting to Kaspa node...");
    let node_config = NodeConfig::default();
    let node_client = match connect_to_node(node_config).await {
        Ok(client) => {
            println!("✅ Connected to Kaspa node successfully");
            Some(client)
        }
        Err(e) => {
            println!("⚠️  Warning: Could not connect to Kaspa node: {e}");
            println!("   The server will continue in offline mode");
            None
        }
    };

    // Initialize server state
    let state = Arc::new(ServerState::new(agent1_wallet, agent2_wallet, node_client));

    // Create a buffer reader for stdin
    let stdin = io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    // Main server loop
    loop {
        tokio::select! {
            result = lines.next_line() => {
                match result {
                    Ok(Some(line)) => { process_request(line, state.clone()).await?; }
                    Ok(None) => { break; }
                    Err(e) => { eprintln!("Error reading line: {e}"); break; }
                }
            }
            _ = signal::ctrl_c() => { println!("Received shutdown signal, exiting..."); break; }
        }
    }

    Ok(())
}

async fn process_request(request_str: String, state: Arc<ServerState>) -> Result<()> {
    // Parse the request into a JSON-RPC Request struct
    let request: Request = match from_str(&request_str) {
        Ok(req) => req,
        Err(e) => {
            let response = Response::error(None, -32700, "Parse error".to_string(), Some(serde_json::Value::String(e.to_string())));
            send_response(response).await?;
            return Ok(());
        }
    };

    // Dispatch
    let response = match request.method.as_str() {
        "tools/list" => crate::rpc_handlers::handle_tools_list(request, state.clone()).await,
        "tools/call" => crate::rpc_handlers::handle_tools_call(request, state.clone()).await,
        _ => Response::error(request.id, -32601, "Method not found".to_string(), None),
    };

    // Send the response
    send_response(response).await?;
    Ok(())
}

async fn send_response(response: Response) -> Result<()> {
    let response_str = to_string(&response)?;
    let stdout = io::stdout();
    let mut writer = stdout;
    writer.write_all(response_str.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
