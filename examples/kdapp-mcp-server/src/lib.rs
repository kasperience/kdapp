// src/lib.rs - Public exports for the kdapp MCP server library

// Re-export modules for public use
pub mod wallet;
pub mod jsonrpc;
pub mod state;
pub mod tools;
pub mod node_connector;

// Re-export key types
pub use wallet::AgentWallet;
pub use jsonrpc::{Request, Response};
pub use state::ServerState;
pub use node_connector::{connect_to_node, NodeConfig};