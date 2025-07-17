use std::error::Error;
use log::info;

// Import CLI module and command handlers
use comment_it::cli::{build_cli, commands::*};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing for better logging
    tracing_subscriber::fmt::init();

    // Use the extracted CLI parser (much cleaner!)
    let matches = build_cli().get_matches();

    match matches.subcommand() {
        Some(("test-episode", sub_matches)) => {
            comment_it::cli::utility_commands::handle_test_episode(sub_matches)?;
        }
        Some(("http-peer", sub_matches)) => {
            comment_it::cli::organizer_commands::handle_http_peer(sub_matches).await?;
        }
        Some(("authenticate", sub_matches)) => {
            comment_it::cli::auth_commands::handle_authenticate(sub_matches).await?;
        }
        Some(("authenticate-full-flow", sub_matches)) => {
            comment_it::cli::auth_commands::handle_authenticate_full_flow(sub_matches).await?;
        }
        Some(("logout", sub_matches)) => {
            comment_it::cli::auth_commands::handle_logout(sub_matches).await?;
        }
        Some(("revoke-session", sub_matches)) => {
            comment_it::cli::auth_commands::handle_revoke_session(sub_matches).await?;
        }
        Some(("submit-comment", sub_matches)) => {
            comment_it::cli::utility_commands::handle_submit_comment(sub_matches).await?;
        }
        Some(("wallet-status", sub_matches)) => {
            comment_it::cli::utility_commands::handle_wallet_status(sub_matches)?;
        }
        Some(("demo", _)) => {
            comment_it::cli::utility_commands::handle_demo()?;
        }
        Some(("organizer-peer", sub_matches)) => {
            comment_it::cli::organizer_commands::handle_organizer_peer(sub_matches).await?;
        }
        Some(("participant-peer", sub_matches)) => {
            comment_it::cli::organizer_commands::handle_participant_peer(sub_matches).await?;
        }
        
        Some(("test-api-flow", sub_matches)) => {
            comment_it::cli::utility_commands::handle_test_api_flow(sub_matches).await?;
        }
        Some(("test-api", sub_matches)) => {
            comment_it::cli::utility_commands::handle_test_api(sub_matches).await?;
        }
        Some(("unified-peer", sub_matches)) => {
            comment_it::cli::organizer_commands::handle_unified_peer(sub_matches).await?;
        }
        _ => {
            println!("No subcommand specified. Use --help for available commands.");
            println!("\nAvailable commands:");
            println!("  authenticate  - 🚀 kdapp authentication (UNIFIED ARCHITECTURE)");
            println!("  test-episode  - Test locally (no Kaspa network)");
            println!("  http-peer     - Run HTTP coordination peer");
            println!("  demo         - Interactive demo (simulated)");
            println!("  organizer-peer - Run auth organizer peer on testnet-10");
            println!("  participant-peer - Run auth participant peer on testnet-10");
        }
    }

    Ok(())
}
