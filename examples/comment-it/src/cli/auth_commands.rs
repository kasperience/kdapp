// src/cli/auth_commands.rs
use std::error::Error;
use clap::ArgMatches;
use crate::wallet::get_wallet_for_command;
use crate::utils::crypto::{parse_private_key, load_private_key_from_file};
use crate::auth::authentication::{run_authentication_with_timeout, run_full_authentication_cycle};
use crate::auth::session::{run_logout_with_timeout, run_session_revocation};

/// Handle authenticate command
pub async fn handle_authenticate(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let peer_url = sub_matches.get_one::<String>("peer").unwrap().clone();
    let use_pure_kdapp = sub_matches.get_flag("pure-kdapp");
    let timeout_seconds: u64 = sub_matches.get_one::<String>("timeout").unwrap().parse()
        .map_err(|_| "Invalid timeout value")?;
    
    // Get private key using unified wallet system
    let _auth_keypair = if let Some(keyfile_path) = sub_matches.get_one::<String>("keyfile") {
        load_private_key_from_file(keyfile_path)?
    } else {
        let provided_private_key = sub_matches.get_one::<String>("key").map(|s| s.as_str());
        let wallet = get_wallet_for_command("authenticate", provided_private_key)?;
        wallet.keypair
    };

    // Get funding keypair for transactions
    let funding_wallet = get_wallet_for_command("participant-peer", None)?;
    let funding_keypair = funding_wallet.keypair;
    
    println!("🔐 Running focused authentication test ({}s timeout)", timeout_seconds);
    
    if use_pure_kdapp {
        println!("🚀 Starting pure kdapp authentication (experimental)");
        println!("⚡ No HTTP coordination - pure peer-to-peer via Kaspa blockchain");
        run_authentication_with_timeout(_auth_keypair, peer_url.clone(), timeout_seconds).await?;
    } else {
        println!("🚀 Starting hybrid authentication (kdapp + HTTP coordination)");
        println!("🎯 Organizer peer: {}", peer_url);
        run_authentication_with_timeout(funding_keypair, peer_url, timeout_seconds).await?;
    }
    
    Ok(())
}

/// Handle authenticate-full-flow command
pub async fn handle_authenticate_full_flow(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let peer_url = sub_matches.get_one::<String>("peer").unwrap().clone();
    let session_duration: u64 = sub_matches.get_one::<String>("session-duration").unwrap().parse()
        .map_err(|_| "Invalid session duration value")?;
    let auth_timeout: u64 = sub_matches.get_one::<String>("auth-timeout").unwrap().parse()
        .map_err(|_| "Invalid auth timeout value")?;
    
    // Get private key using unified wallet system
    let _auth_keypair = if let Some(keyfile_path) = sub_matches.get_one::<String>("keyfile") {
        load_private_key_from_file(keyfile_path)?
    } else {
        let provided_private_key = sub_matches.get_one::<String>("key").map(|s| s.as_str());
        let wallet = get_wallet_for_command("authenticate", provided_private_key)?;
        wallet.keypair
    };

    // Get funding keypair for transactions
    let funding_wallet = get_wallet_for_command("participant-peer", None)?;
    let funding_keypair = funding_wallet.keypair;
    
    println!("🔄 Running complete authentication lifecycle test");
    println!("⏱️  Auth timeout: {}s, Session duration: {}s", auth_timeout, session_duration);
    println!("🎯 Organizer peer: {}", peer_url);
    
    run_full_authentication_cycle(funding_keypair, peer_url).await?;
    
    Ok(())
}

/// Handle logout command
pub async fn handle_logout(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let episode_id: u64 = sub_matches
        .get_one::<String>("episode-id")
        .unwrap()
        .parse()
        .map_err(|_| "Invalid episode ID")?;
    
    let session_token = sub_matches
        .get_one::<String>("session-token")
        .unwrap()
        .clone();
    
    let peer_url = sub_matches.get_one::<String>("peer").unwrap().clone();
    let timeout_seconds: u64 = sub_matches.get_one::<String>("timeout").unwrap().parse()
        .map_err(|_| "Invalid timeout value")?;
    
    // Get private key using unified wallet system
    let _auth_keypair = if let Some(provided_private_key) = sub_matches.get_one::<String>("key") {
        parse_private_key(provided_private_key)?
    } else {
        let wallet = get_wallet_for_command("participant-peer", None)?;
        wallet.keypair
    };
    
    println!("🚪 Running focused logout test ({}s timeout)", timeout_seconds);
    println!("📋 Episode: {}, Session: {}", episode_id, session_token);
    
    run_logout_with_timeout(_auth_keypair, episode_id, session_token, peer_url, timeout_seconds).await?;
    
    Ok(())
}

/// Handle revoke-session command
pub async fn handle_revoke_session(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let episode_id: u64 = sub_matches
        .get_one::<String>("episode-id")
        .unwrap()
        .parse()
        .map_err(|_| "Invalid episode ID")?;
    
    let session_token = sub_matches
        .get_one::<String>("session-token")
        .unwrap()
        .clone();
    
    let peer_url = sub_matches.get_one::<String>("peer").unwrap().clone();
    
    // Get private key using unified wallet system
    let _auth_keypair = if let Some(provided_private_key) = sub_matches.get_one::<String>("key") {
        parse_private_key(provided_private_key)?
    } else {
        let wallet = get_wallet_for_command("participant-peer", None)?;
        wallet.keypair
    };
    
    println!("🔄 Running session revocation (blockchain transaction)");
    run_session_revocation(_auth_keypair, episode_id, session_token, peer_url).await?;
    
    Ok(())
}

// Helper functions are now imported at the top of the file