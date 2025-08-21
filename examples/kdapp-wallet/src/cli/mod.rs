use crate::wallet;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new wallet
    Create {
        /// Store the private key in a local file for development purposes (INSECURE!)
        #[arg(long)]
        dev_mode: bool,
    },
    /// Get a wallet address
    Address {
        /// Use development mode (read key from file)
        #[arg(long)]
        dev_mode: bool,
    },
    /// Get wallet balance
    /// Get wallet balance
    Balance {
        /// Optional: RPC URL of the Kaspa node (e.g., "grpc://127.0.0.1:16110")
        #[arg(long)]
        rpc_url: Option<String>,
        /// Use development mode (read key from file)
        #[arg(long)]
        dev_mode: bool,
    },
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Create { dev_mode } => {
            wallet::create_wallet(*dev_mode).await?;
        }
        Commands::Address { dev_mode } => {
            wallet::get_address(*dev_mode).await?;
        }
        Commands::Balance { rpc_url, dev_mode } => {
            wallet::get_balance(rpc_url.clone(), *dev_mode).await?;
        }
    }

    Ok(())
}
