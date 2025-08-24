use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "Pure kdapp Comment Board - Based on TicTacToe Architecture", long_about = None)]
pub struct Args {
    /// Kaspa schnorr private key (pays for your transactions)
    #[arg(short, long)]
    pub kaspa_private_key: Option<String>,

    /// Room episode ID to join (optional - creates new room if not provided)
    #[arg(short = 'r', long)]
    pub room_episode_id: Option<u32>,

    /// Indicates whether to run the interaction over mainnet (default: testnet 10)
    #[arg(short, long, default_value_t = false)]
    pub mainnet: bool,

    /// Specifies the wRPC Kaspa Node URL to use. Usage: <wss://localhost>. Defaults to the Public Node Network (PNN).
    #[arg(short, long)]
    pub wrpc_url: Option<String>,

    /// Logging level for all subsystems {off, error, warn, info, debug, trace}
    ///  -- You may also specify `<subsystem>=<level>,<subsystem2>=<level>,...` to set the log level for individual subsystems
    #[arg(long = "loglevel", default_value = format!("info,{}=trace", env!("CARGO_PKG_NAME")))]
    pub log_level: String,

    /// Forbidden words for room moderation (comma-separated, e.g., "fuck,shit,damn")
    #[arg(long)]
    pub forbidden_words: Option<String>,

    /// Enable economic comment bonds (users pay 100 KAS to comment)
    #[arg(long, default_value_t = false)]
    pub bonds: bool,

    /// Experimental: use script-based bond output in the combined tx (may be non-standard)
    #[arg(long, default_value_t = false)]
    pub script_bonds: bool,

    /// Optional: declare a bond script descriptor policy for episode-side verification
    /// Supported: "p2pk", "timelock" (others reserved)
    #[arg(long)]
    pub bond_script_descriptor: Option<String>,

    /// Optional: bond amount in KAS for --bonds posts (overrides default/room min)
    #[arg(long)]
    pub bond_amount: Option<f64>,

    /// Optional: organizer-preferred minimum bond in KAS (used by this client when --bonds is set)
    #[arg(long)]
    pub min_bond: Option<f64>,
}
