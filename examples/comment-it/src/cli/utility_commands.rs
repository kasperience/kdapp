// src/cli/utility_commands.rs
use std::error::Error;
use clap::ArgMatches;

/// Handle test-episode command
pub fn handle_test_episode(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let participant_count: usize = sub_matches
        .get_one::<String>("participants")
        .unwrap()
        .parse()
        .unwrap_or(1);
    
    test_episode_logic(participant_count)?;
    Ok(())
}

/// Handle demo command  
pub fn handle_demo() -> Result<(), Box<dyn Error>> {
    run_interactive_demo()?;
    Ok(())
}

/// Handle wallet-status command
pub fn handle_wallet_status(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let role = sub_matches.get_one::<String>("role").unwrap();
    show_wallet_status(role)?;
    Ok(())
}

/// Handle submit-comment command
pub async fn handle_submit_comment(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let episode_id: u64 = sub_matches
        .get_one::<String>("episode-id")
        .unwrap()
        .parse()
        .map_err(|_| "Invalid episode ID")?;
    
    let comment_text = sub_matches
        .get_one::<String>("text")
        .unwrap()
        .clone();
    
    let session_token = sub_matches
        .get_one::<String>("session-token")
        .unwrap()
        .clone();
    
    let private_key = sub_matches.get_one::<String>("key").map(|s| s.as_str());
    
    crate::cli::commands::submit_comment::run_submit_comment_command(
        episode_id,
        comment_text,
        session_token,
        None, // kaspa_address
        private_key,
    ).await?;
    
    Ok(())
}

/// Handle test-api-flow command
pub async fn handle_test_api_flow(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let peer_url = sub_matches.get_one::<String>("peer").unwrap().clone();
    let command = crate::cli::commands::test_api_flow::TestApiFlowCommand { peer: peer_url };
    command.execute().await?;
    Ok(())
}

/// Handle test-api command
pub async fn handle_test_api(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let peer_url = sub_matches.get_one::<String>("peer").unwrap().clone();
    let command = crate::cli::commands::test_api::TestApiCommand { 
        peer: peer_url, 
        verbose: false, 
        json: false 
    };
    command.execute().await?;
    Ok(())
}

// These functions need to be moved from main.rs
use crate::cli::commands::demo::{test_episode_logic, run_interactive_demo};

use crate::wallet::get_wallet_for_command;

fn show_wallet_status(role: &str) -> Result<(), Box<dyn Error>> {
    use std::path::Path;
    
    println!("🔍 Kaspa Auth Wallet Status Report");
    println!("==================================");
    
    let wallet_dir = Path::new(".kaspa-auth");
    
    if !wallet_dir.exists() {
        println!("❌ No .kaspa-auth directory found");
        println!("💡 Run any command to create initial wallets");
        return Ok(());
    }
    
    match role {
        "all" => {
            check_wallet_role("organizer-peer");
            println!();
            check_wallet_role("participant-peer");
        },
        role => check_wallet_role(role),
    }
    
    println!();
    println!("🚰 Testnet Faucet: https://faucet.kaspanet.io/");
    println!("🔍 Explorer: https://explorer.kaspanet.io/");
    
    Ok(())
}

fn check_wallet_role(role: &str) {
    use std::path::Path;
    
    let wallet_file = Path::new(".kaspa-auth").join(format!("{}-wallet.key", role));
    
    println!("🔑 {} Wallet:", role.to_uppercase());
    
    if wallet_file.exists() {
        // Try to load the wallet to get address info
        match get_wallet_for_command(role, None) {
            Ok(wallet) => {
                let kaspa_addr = wallet.get_kaspa_address();
                let file_size = std::fs::metadata(&wallet_file)
                    .map(|m| m.len())
                    .unwrap_or(0);
                
                println!("  ✅ Status: EXISTS and LOADED");
                println!("  📁 File: {}", wallet_file.display());
                println!("  📊 Size: {} bytes", file_size);
                println!("  🏠 Address: {}", kaspa_addr);
                println!("  🔄 Will be REUSED on next run");
            }
            Err(e) => {
                println!("  ❌ Status: EXISTS but CORRUPTED");
                println!("  📁 File: {}", wallet_file.display());
                println!("  ⚠️  Error: {}", e);
                println!("  🔧 Solution: Delete file to recreate");
            }
        }
    } else {
        println!("  ❓ Status: NOT CREATED YET");
        println!("  📁 Will create: {}", wallet_file.display());
        println!("  🆕 Will be NEW on next run");
    }
}