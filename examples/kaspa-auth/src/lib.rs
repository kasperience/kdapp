// Core working modules
pub mod api;
pub mod core;
pub mod crypto;
pub mod episode_runner;

// Framework modules (re-enable anytime)
pub mod auth;
pub mod cli;
pub mod daemon;
pub mod utils;
pub mod wallet;

// Future modules (moved to future examples)
// pub mod commitments;     // → kaspa-poker-tournament
// pub mod economics;       // → kaspa-poker-tournament
// pub mod oracle;          // → episode-contract
// pub mod time_bounded_auth; // → episode-contract
// pub mod state_management; // → episode-contract
// pub mod network;         // → future networking example
// pub mod storage;         // → future storage example
// pub mod examples;        // → individual example projects

// Public API exports (only working functionality)
pub use auth::{run_http_coordinated_authentication, run_session_revocation, AuthenticationResult};
pub use core::commands::AuthCommand;
pub use episode_runner::{create_auth_generator, run_auth_organizer_peer, AuthEventHandler, AuthOrganizerConfig};
