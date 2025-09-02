use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "kas-draw", version, about = "Kaspa lottery episode (M1 MVP)")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    New {
        #[arg(long)]
        episode_id: u32,
    },
    Buy {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        amount: u64,
        #[arg(value_name = "N", num_args = 5)]
        numbers: Vec<u8>,
    },
    Draw {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        entropy: String,
    },
    Claim {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        ticket_id: u64,
        #[arg(long)]
        round: u64,
    },
    /// Submit a Checkpoint payload to L1 (OKCP v1)
    SubmitCheckpoint {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        seq: u64,
        #[arg(long)]
        state_root: String,
        #[arg(long)]
        kaspa_private_key: Option<String>,
        #[arg(long)]
        mainnet: bool,
        #[arg(long)]
        wrpc_url: Option<String>,
    },
    /// Start engine + proxy listener (L1 mode). Stop with Ctrl+C.
    Engine {
        #[arg(long)]
        mainnet: bool,
        #[arg(long)]
        wrpc_url: Option<String>,
    },
    /// Start off-chain engine + in-proc UDP router. Stop with Ctrl+C.
    OffchainEngine {
        #[arg(long, default_value_t = String::from("127.0.0.1:18181"))]
        bind: String,
        #[arg(long)]
        no_ack: bool,
        #[arg(long)]
        no_close: bool,
    },
    /// Submit a NewEpisode transaction carrying participants (your pubkey)
    SubmitNew {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        kaspa_private_key: Option<String>,
        #[arg(long)]
        mainnet: bool,
        #[arg(long)]
        wrpc_url: Option<String>,
    },
    /// Submit a BuyTicket transaction
    SubmitBuy {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        kaspa_private_key: Option<String>,
        #[arg(long)]
        mainnet: bool,
        #[arg(long)]
        wrpc_url: Option<String>,
        #[arg(long)]
        amount: u64,
        #[arg(value_name = "N", num_args = 5)]
        numbers: Vec<u8>,
    },
    /// Submit a Draw transaction
    SubmitDraw {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        kaspa_private_key: Option<String>,
        #[arg(long)]
        mainnet: bool,
        #[arg(long)]
        wrpc_url: Option<String>,
        #[arg(long)]
        entropy: String,
    },
    /// Submit a Claim transaction
    SubmitClaim {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        kaspa_private_key: Option<String>,
        #[arg(long)]
        mainnet: bool,
        #[arg(long)]
        wrpc_url: Option<String>,
        #[arg(long)]
        ticket_id: u64,
        #[arg(long)]
        round: u64,
    },
    /// Send a TLV v1 message to the off-chain router
    OffchainSend {
        #[arg(long)]
        r#type: String, // new|cmd|close|ckpt
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        router: Option<String>,
        #[arg(long)]
        force_seq: Option<u64>,
        #[arg(long)]
        no_ack: bool,
        #[arg(long)]
        kaspa_private_key: Option<String>,
        // For Buy
        #[arg(long)]
        amount: Option<u64>,
        #[arg(value_name = "N", num_args = 5)]
        numbers: Vec<u8>,
        // For Draw
        #[arg(long)]
        entropy: Option<String>,
        // For Claim
        #[arg(long)]
        ticket_id: Option<u64>,
        #[arg(long)]
        round: Option<u64>,
        // For Checkpoint
        #[arg(long)]
        state_root: Option<String>,
    },
}

