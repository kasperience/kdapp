// src/lib.rs - Public exports for the kdapp MCP server library

// Re-export modules for public use
pub mod jsonrpc;
pub mod node_connector;
pub mod routing;
pub mod state;
pub mod tools;
pub mod wallet;

// Re-export key types
pub use jsonrpc::{Request, Response};
pub use node_connector::{connect_to_node, NodeConfig};
pub use state::ServerState;
pub use wallet::AgentWallet;
