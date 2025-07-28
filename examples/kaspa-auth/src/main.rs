use std::error::Error;
use clap::Parser;
use kaspa_auth::cli::Cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing for better logging
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    
    // Show keychain usage info if enabled
    if cli.keychain {
        println!("🔐 Using OS keychain for secure wallet storage");
        if cli.dev_mode {
            println!("⚠️  Development mode: Using insecure local files instead of keychain");
        }
    } else {
        println!("📁 Using file-based wallet storage (.kaspa-auth/ directory)");
    }
    
    cli.command.execute(cli.keychain, cli.dev_mode).await?;
    Ok(())
}