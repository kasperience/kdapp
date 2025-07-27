use clap::{Parser, Subcommand};
use crate::episode::{RoomRules, ViolationType, ContractCommand};
use kdapp::pki::PubKey;

/// Enhanced CLI for Episode Contract Features
#[derive(Parser, Debug)]
#[command(author, version, about = "Kaspa Episode Contract Comment Board - Revolutionary Decentralized Moderation", long_about = None)]
pub struct ContractArgs {
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
    #[arg(long = "loglevel", default_value = format!("info,{}=trace", env!("CARGO_PKG_NAME")))]
    pub log_level: String,

    /// Episode contract command to execute
    #[command(subcommand)]
    pub command: Option<ContractCommand>,

    /// Interactive mode (default if no command provided)
    #[arg(long, default_value_t = true)]
    pub interactive: bool,
}

/// Terminal Commands for Episode Contract Showcase
#[derive(Subcommand, Debug, Clone)]
pub enum TerminalCommand {
    /// Create a new comment room with custom rules and economic parameters
    CreateRoom {
        /// Minimum bond amount in sompi (default: 100 = 0.000001 KAS)
        #[arg(long, default_value_t = 100)]
        min_bond: u64,
        
        /// Maximum comment length (default: 500)
        #[arg(long, default_value_t = 500)]
        max_length: usize,
        
        /// Penalty multiplier for violations (default: 2.0)
        #[arg(long, default_value_t = 2.0)]
        penalty_multiplier: f64,
        
        /// Enable community moderation voting
        #[arg(long, default_value_t = true)]
        community_moderation: bool,
        
        /// Initial funding for the room in sompi
        #[arg(long, default_value_t = 10000)]
        initial_funding: u64,
        
        /// Custom episode lifetime in seconds (default: 7776000 = 3 months)
        #[arg(long)]
        custom_lifetime: Option<u64>,
        
        /// Moderator public keys (comma-separated)
        #[arg(long)]
        moderators: Option<String>,
        
        /// Forbidden words (comma-separated, e.g., "fuck,shit,damn,spam,scam")
        #[arg(long)]
        forbidden_words: Option<String>,
    },
    
    /// Join an existing room with a participation bond
    JoinRoom {
        /// Bond amount to lock for participation (must meet room minimum)
        #[arg(long, default_value_t = 100)]
        bond_amount: u64,
    },
    
    /// Submit a comment with economic bond
    Comment {
        /// Comment text
        text: String,
        
        /// Bond amount for this comment (higher bonds show more confidence)
        #[arg(long, default_value_t = 100)]
        bond: u64,
    },
    
    /// Report a violation for community moderation
    ReportViolation {
        /// Comment ID to report
        comment_id: u64,
        
        /// Type of violation
        #[arg(value_enum)]
        violation_type: CliViolationType,
        
        /// Evidence/reason for the report
        evidence: String,
    },
    
    /// Vote on a comment quality or violation report
    Vote {
        /// Vote ID
        vote_id: u64,
        
        /// Vote decision
        #[arg(value_enum)]
        decision: CliVoteDecision,
        
        /// KAS amount to stake on this vote (shows confidence)
        #[arg(long, default_value_t = 50)]
        stake: u64,
    },
    
    /// Upvote a comment (stakes KAS on quality)
    Upvote {
        /// Comment ID to upvote
        comment_id: u64,
        
        /// KAS to stake on this upvote
        #[arg(long, default_value_t = 10)]
        stake: u64,
    },
    
    /// Downvote a comment (stakes KAS on poor quality)
    Downvote {
        /// Comment ID to downvote
        comment_id: u64,
        
        /// KAS to stake on this downvote
        #[arg(long, default_value_t = 10)]
        stake: u64,
    },
    
    /// Claim bond refund for a comment after lock period
    ClaimRefund {
        /// Comment ID to claim refund for
        comment_id: u64,
    },
    
    /// Display current episode contract statistics
    Stats,
    
    /// Show reputation leaderboard
    Leaderboard,
    
    /// Display user's reputation and bond status
    MyStatus,
    
    /// Generate Twitter showcase message
    Tweet,
    
    /// Show contract rules and parameters
    Rules,
    
    /// Interactive comment mode (like original)
    Interactive,
}

/// CLI-friendly violation types
#[derive(clap::ValueEnum, Debug, Clone)]
pub enum CliViolationType {
    Spam,
    OffTopic, 
    Harassment,
    Inappropriate,
    Misinformation,
    Copyright,
}

impl From<CliViolationType> for ViolationType {
    fn from(cli_type: CliViolationType) -> Self {
        match cli_type {
            CliViolationType::Spam => ViolationType::Spam,
            CliViolationType::OffTopic => ViolationType::OffTopic,
            CliViolationType::Harassment => ViolationType::Harassment,
            CliViolationType::Inappropriate => ViolationType::InappropriateContent,
            CliViolationType::Misinformation => ViolationType::Misinformation,
            CliViolationType::Copyright => ViolationType::CopyrightViolation,
        }
    }
}

/// CLI-friendly vote decisions
#[derive(clap::ValueEnum, Debug, Clone)]
pub enum CliVoteDecision {
    Approve,
    Penalize,
    Escalate,
    Dismiss,
}

impl From<CliVoteDecision> for crate::episode::commands::VoteDecision {
    fn from(cli_decision: CliVoteDecision) -> Self {
        match cli_decision {
            CliVoteDecision::Approve => crate::episode::commands::VoteDecision::Approve,
            CliVoteDecision::Penalize => crate::episode::commands::VoteDecision::Penalize,
            CliVoteDecision::Escalate => crate::episode::commands::VoteDecision::Escalate,
            CliVoteDecision::Dismiss => crate::episode::commands::VoteDecision::Dismiss,
        }
    }
}

/// Convert CLI arguments to RoomRules
impl From<&TerminalCommand> for Option<RoomRules> {
    fn from(cmd: &TerminalCommand) -> Self {
        match cmd {
            TerminalCommand::CreateRoom { 
                min_bond, 
                max_length, 
                penalty_multiplier, 
                community_moderation,
                forbidden_words,
                ..
            } => {
                let forbidden_list = forbidden_words
                    .as_ref()
                    .map(|words| words.split(',').map(|w| w.trim().to_string()).collect())
                    .unwrap_or_default();
                    
                Some(RoomRules {
                    min_bond: *min_bond,
                    max_bond: min_bond * 100, // Default max is 100x min
                    penalty_multiplier: *penalty_multiplier,
                    reward_pool_percentage: 0.8,
                    max_comment_length: *max_length,
                    min_reputation_threshold: -50,
                    spam_detection_enabled: true,
                    forbidden_words: forbidden_list,
                    community_moderation: *community_moderation,
                    auto_penalty_enabled: true,
                    dispute_resolution_timeout: 172800, // 2 days
                    voting_period: 86400, // 1 day
                    reputation_decay_rate: 0.01,
                    dynamic_bond_pricing: true,
                    quality_bonus_enabled: true,
                })
            }
            _ => None,
        }
    }
}

/// Parse moderator public keys from comma-separated string
pub fn parse_moderators(moderators_str: Option<&String>) -> Vec<PubKey> {
    moderators_str
        .map(|s| {
            s.split(',')
                .map(|key_str| {
                    // This is a simplified parser - real implementation would validate pubkey format
                    // For now, create a dummy PubKey from the string
                    let mut bytes = [0u8; 33];
                    let key_bytes = key_str.trim().as_bytes();
                    let len = key_bytes.len().min(33);
                    bytes[..len].copy_from_slice(&key_bytes[..len]);
                    PubKey(bytes)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Display helper functions for terminal output
pub mod display {
    use crate::episode::{ContractStats, format_kas_amount, format_reputation, generate_showcase_message};
    
    /// Display contract statistics in terminal-friendly format
    pub fn show_contract_stats(stats: &ContractStats) {
        println!("╔══════════════════════════════════════════════════════════════════════════════╗");
        println!("║                          🏛️ EPISODE CONTRACT STATS                            ║");
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");
        println!("║ 💰 Total Locked Value: {:>20}                                    ║", 
                 format_kas_amount(stats.total_locked_kas));
        println!("║ 👥 Active Participants: {:>19}                                    ║", 
                 stats.active_participants);
        println!("║ 💬 Total Comments: {:>24}                                    ║", 
                 stats.total_comments);
        println!("║ ⚖️ Active Disputes: {:>23}                                    ║", 
                 stats.active_disputes);
        println!("║ 🏆 Penalty Pool: {:>26}                                    ║", 
                 format_kas_amount(stats.penalty_pool_kas));
        println!("║ 🎯 Avg Reputation: {:>25}                                    ║", 
                 format_reputation(stats.average_reputation as i32));
        println!("║ ⏰ Contract Uptime: {:>21} hours                                ║", 
                 stats.contract_uptime_hours);
        println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    }
    
    /// Generate Twitter showcase message
    pub fn generate_twitter_showcase(stats: &ContractStats) -> String {
        generate_showcase_message(stats)
    }
    
    /// Display room rules in readable format
    pub fn show_room_rules(rules: &crate::episode::RoomRules) {
        println!("╔══════════════════════════════════════════════════════════════════════════════╗");
        println!("║                              🏛️ ROOM RULES                                    ║");
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");
        println!("║ 💰 Min Bond: {} | Max Bond: {}          ║", 
                 format_kas_amount(rules.min_bond), 
                 format_kas_amount(rules.max_bond));
        println!("║ ⚖️ Penalty Multiplier: {:.1}x                                                ║", 
                 rules.penalty_multiplier);
        println!("║ 📝 Max Comment Length: {} chars                                             ║", 
                 rules.max_comment_length);
        println!("║ 🎯 Min Reputation: {}                                                      ║", 
                 format_reputation(rules.min_reputation_threshold));
        println!("║ 🗳️ Community Moderation: {}                                                ║", 
                 if rules.community_moderation { "✅ Enabled" } else { "❌ Disabled" });
        println!("║ 🤖 Auto Penalties: {}                                                     ║", 
                 if rules.auto_penalty_enabled { "✅ Enabled" } else { "❌ Disabled" });
        println!("║ ⏰ Voting Period: {} hours                                                   ║", 
                 rules.voting_period / 3600);
        println!("║ 🎁 Quality Bonuses: {}                                                     ║", 
                 if rules.quality_bonus_enabled { "✅ Enabled" } else { "❌ Disabled" });
        
        if !rules.forbidden_words.is_empty() {
            println!("║ 🚫 Forbidden Words: {}                                              ║", 
                     rules.forbidden_words.join(", "));
        }
        
        println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    }
    
    /// Display user status
    pub fn show_user_status(reputation: i32, locked_bonds: u64, available_balance: u64) {
        println!("╔══════════════════════════════════════════════════════════════════════════════╗");
        println!("║                               👤 YOUR STATUS                                  ║");
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");
        println!("║ 🎯 Reputation: {}                                                        ║", 
                 format_reputation(reputation));
        println!("║ 🔒 Locked in Bonds: {}                                              ║", 
                 format_kas_amount(locked_bonds));
        println!("║ 💰 Available Balance: {}                                            ║", 
                 format_kas_amount(available_balance));
        println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    }
}

/// Terminal interaction helpers
pub mod interaction {
    use std::io::{self, Write};
    
    /// Get user input with prompt
    pub fn get_input(prompt: &str) -> String {
        print!("{}", prompt);
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string()
    }
    
    /// Get confirmed user input for important actions
    pub fn get_confirmed_input(prompt: &str, confirmation_msg: &str) -> Option<String> {
        let input = get_input(prompt);
        if input.is_empty() {
            return None;
        }
        
        let confirm = get_input(&format!("{} (y/N): ", confirmation_msg));
        if confirm.to_lowercase() == "y" || confirm.to_lowercase() == "yes" {
            Some(input)
        } else {
            None
        }
    }
    
    /// Show menu and get selection
    pub fn show_menu(title: &str, options: &[&str]) -> Option<usize> {
        println!("\n╔════════════════════════════════════════════════════════════════════════════╗");
        println!("║ {} ║", title);
        println!("╠════════════════════════════════════════════════════════════════════════════╣");
        
        for (i, option) in options.iter().enumerate() {
            println!("║ {}. {} ║", i + 1, option);
        }
        
        println!("║ 0. Exit ║");
        println!("╚════════════════════════════════════════════════════════════════════════════╝");
        
        let input = get_input("Select option: ");
        match input.parse::<usize>() {
            Ok(0) => None,
            Ok(n) if n <= options.len() => Some(n - 1),
            _ => {
                println!("❌ Invalid selection. Please try again.");
                None
            }
        }
    }
}