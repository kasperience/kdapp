pub mod episode;
pub mod commands;
pub mod errors;
pub mod types;
pub mod commitment_reveal;

pub use episode::{AuthWithCommentsEpisode, Comment};
pub use commands::{UnifiedCommand};
// Backward compatibility aliases

pub use errors::AuthError;
pub use types::{AuthRollback, UnifiedRollback, AuthState, AuthRole};
pub use commitment_reveal::CommitRevealChallenge;