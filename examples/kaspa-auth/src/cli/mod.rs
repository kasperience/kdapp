pub mod commands;
pub mod config;
pub mod utils;

use clap::{Parser, Subcommand};
use commands::*;

#[derive(Parser)]
#[command(name = "kaspa-auth")]
#[command(version = "0.1.0")]
#[command(about = "Kaspa Authentication Episode Demo")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Use OS keychain for secure wallet storage (instead of file-based)
    #[arg(long, global = true)]
    pub keychain: bool,

    /// Development mode: store keys insecurely in local files (DO NOT USE FOR REAL FUNDS)
    #[arg(long, global = true)]
    pub dev_mode: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Test auth episode locally (no Kaspa)
    TestEpisode(test::TestEpisodeCommand),
    /// Run HTTP coordination organizer peer for authentication
    HttpOrganizerPeer(http_organizer_peer::HttpOrganizerPeerCommand),
    /// 🚀 One-command authentication with HTTP organizer peer (EASY MODE)
    Authenticate(authenticate::AuthenticateCommand),
    /// 🔄 Complete login → session → logout cycle with timeouts
    AuthenticateFullFlow(authenticate_full_flow::AuthenticateFullFlowCommand),
    /// Run interactive demo
    Demo(demo::DemoCommand),
    /// Run auth organizer peer on Kaspa testnet-10
    OrganizerPeer(organizer_peer::OrganizerPeerCommand),
    /// Run auth participant peer on Kaspa testnet-10
    ParticipantPeer(participant_peer::ParticipantPeerCommand),
    /// Check wallet status in OS keychain
    WalletStatus(wallet_status::WalletStatusCommand),
    /// Manage kaspa-auth daemon service
    Daemon(daemon::DaemonCommand),
}

impl Commands {
    pub async fn execute(self, keychain: bool, dev_mode: bool) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Commands::TestEpisode(cmd) => cmd.execute().await,
            Commands::HttpOrganizerPeer(mut cmd) => {
                cmd.set_storage_options(keychain, dev_mode);
                cmd.execute().await
            }
            Commands::Authenticate(mut cmd) => {
                cmd.set_storage_options(keychain, dev_mode);
                cmd.execute().await
            }
            Commands::AuthenticateFullFlow(mut cmd) => {
                cmd.set_storage_options(keychain, dev_mode);
                cmd.execute().await
            }
            Commands::Demo(cmd) => cmd.execute().await,
            Commands::OrganizerPeer(mut cmd) => {
                cmd.set_storage_options(keychain, dev_mode);
                cmd.execute().await
            }
            Commands::ParticipantPeer(mut cmd) => {
                cmd.set_storage_options(keychain, dev_mode);
                cmd.execute().await
            }
            Commands::WalletStatus(mut cmd) => {
                cmd.set_storage_options(keychain, dev_mode);
                cmd.execute().await
            }
            Commands::Daemon(mut cmd) => {
                cmd.set_storage_options(keychain, dev_mode);
                cmd.execute().await
            }
        }
    }
}
