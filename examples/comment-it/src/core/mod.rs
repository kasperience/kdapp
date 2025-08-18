pub mod commands;
pub mod commitment_reveal;
pub mod episode;
pub mod errors;
pub mod types;

pub use commands::UnifiedCommand;
pub use episode::{AuthWithCommentsEpisode, Comment};
// Backward compatibility aliases

pub use commitment_reveal::CommitRevealChallenge;
pub use errors::AuthError;
pub use types::{AuthRole, AuthRollback, AuthState, UnifiedRollback};
