// src/cli/organizer_commands.rs - Organizer peer commands (renamed from server_commands.rs for P2P philosophy)
use crate::api::http::organizer_peer::run_http_peer;
use crate::auth::authentication::run_http_coordinated_authentication;
use crate::utils::crypto::{generate_random_keypair, load_private_key_from_file, parse_private_key};
use crate::wallet::get_wallet_for_command;
use clap::ArgMatches;
use secp256k1::Keypair;
use std::error::Error;

/// Handle http-peer command
pub async fn handle_http_peer(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let port: u16 = sub_matches.get_one::<String>("port").unwrap().parse().unwrap_or(8080);

    let provided_private_key = sub_matches.get_one::<String>("key").map(|s| s.as_str());
    run_http_peer(provided_private_key, port).await?;

    Ok(())
}

/// Handle organizer-peer command
pub async fn handle_organizer_peer(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let name = sub_matches.get_one::<String>("name").unwrap().clone();
    let rpc_url = sub_matches.get_one::<String>("rpc-url").cloned();
    let provided_private_key = sub_matches.get_one::<String>("key").map(|s| s.as_str());

    let wallet = get_wallet_for_command("organizer-peer", provided_private_key)?;
    run_kaspa_organizer_peer(wallet.keypair, name, rpc_url).await?;

    Ok(())
}

/// Handle participant-peer command
pub async fn handle_participant_peer(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let should_auth = sub_matches.get_flag("auth");
    let rpc_url = sub_matches.get_one::<String>("rpc-url").cloned();

    // Get Kaspa keypair (for funding transactions)
    let kaspa_keypair = if let Some(kaspa_keyfile_path) = sub_matches.get_one::<String>("kaspa-keyfile") {
        load_private_key_from_file(kaspa_keyfile_path)?
    } else if let Some(kaspa_key_hex) = sub_matches.get_one::<String>("kaspa-private-key") {
        parse_private_key(kaspa_key_hex)?
    } else if should_auth {
        // If doing auth and no kaspa key provided, show how to generate one
        let keypair = generate_random_keypair();
        let kaspa_addr = kaspa_addresses::Address::new(
            kaspa_addresses::Prefix::Testnet,
            kaspa_addresses::Version::PubKey,
            &keypair.x_only_public_key().0.serialize(),
        );
        println!("🔑 No --kaspa-private-key or --kaspa-keyfile provided. Generated new participant peer wallet:");
        println!("📍 Kaspa Address: {kaspa_addr}");
        println!("🔐 Private Key: {}", hex::encode(keypair.secret_key().secret_bytes()));
        println!();
        println!("💾 Save the private key to a file for security:");
        println!("echo '{}' > kaspa_private.key", hex::encode(keypair.secret_key().secret_bytes()));
        println!();
        println!("💰 FUNDING REQUIRED: Get testnet Kaspa for blockchain authentication");
        println!("🚰 Faucet URL: https://faucet.kaspanet.io/");
        println!("🌐 Network: testnet-10 (for development and testing)");
        println!("💡 Amount needed: ~0.1 KAS (covers multiple authentication transactions)");
        println!();
        println!("📋 Steps to fund your participant peer wallet:");
        println!("  1. Copy the Kaspa address above: {kaspa_addr}");
        println!("  2. Visit: https://faucet.kaspanet.io/");
        println!("  3. Paste the address and request testnet funds");
        println!("  4. Wait ~30 seconds for transaction confirmation");
        println!();
        println!("🚀 After funding, run blockchain authentication:");
        println!("cargo run -p kaspa-auth -- participant-peer --auth --kaspa-keyfile kaspa_private.key");
        println!("or");
        println!(
            "cargo run -p kaspa-auth -- participant-peer --auth --kaspa-private-key {}",
            hex::encode(keypair.secret_key().secret_bytes())
        );
        println!();
        println!("🎯 This will create REAL blockchain transactions on Kaspa testnet-10!");
        println!("📊 You can verify transactions at: https://explorer.kaspa.org/");
        return Ok(());
    } else {
        generate_random_keypair()
    };

    // Get auth keypair (for episode authentication)
    let provided_private_key = sub_matches.get_one::<String>("key").map(|s| s.as_str());
    let wallet = get_wallet_for_command("participant-peer", provided_private_key)?;

    run_kaspa_participant_peer(kaspa_keypair, wallet.keypair, should_auth, rpc_url).await?;

    Ok(())
}

/// Handle unified-peer command
pub async fn handle_unified_peer(sub_matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let host = sub_matches.get_one::<String>("host").unwrap().clone();
    let port: u16 = sub_matches.get_one::<String>("port").unwrap().parse()?;

    println!("🚀 Starting unified comment-it organizer peer...");

    let organizer = crate::organizer::CommentOrganizer::new(host, port).await?;
    organizer.run().await?;

    Ok(())
}

// Helper functions are now imported at the top of the file

async fn run_kaspa_organizer_peer(_signer: Keypair, name: String, rpc_url: Option<String>) -> Result<(), Box<dyn Error>> {
    println!("🎯 Starting Kaspa Auth Organizer Peer: {name}");
    if let Some(url) = &rpc_url {
        println!("📡 Connecting to node: {url}");
    } else {
        println!("📡 Connecting to testnet-10 (public node)...");
    }

    // TODO: Implement running Kaspa organizer peer without HTTP server
    // For now, this function does nothing.

    Ok(())
}

async fn run_kaspa_participant_peer(
    kaspa_signer: Keypair,
    auth_signer: Keypair,
    should_auth: bool,
    rpc_url: Option<String>,
) -> Result<(), Box<dyn Error>> {
    println!("🔑 Starting Kaspa Auth Participant Peer");
    if let Some(url) = &rpc_url {
        println!("📡 Connecting to node: {url}");
    } else {
        println!("📡 Connecting to testnet-10 (public node)...");
    }

    if should_auth {
        println!("🚀 Initiating blockchain authentication flow...");
        println!("🎯 This will create REAL transactions on Kaspa testnet-10");
        run_http_coordinated_authentication(kaspa_signer, auth_signer, "http://localhost:8080".to_string()).await?;
    } else {
        println!("👂 Participant peer mode: Listening for authentication requests...");
        println!("💡 Tip: Add --auth flag to initiate authentication instead of listening");
        println!("📖 Example: cargo run -- participant-peer --auth --kaspa-keyfile your_key.txt");
        println!();
        // For now, just run a server instance
        // TODO: Implement running Kaspa participant peer without HTTP server
        // For now, this function does nothing.
    }

    Ok(())
}
