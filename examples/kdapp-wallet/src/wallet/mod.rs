use anyhow::{Result, anyhow};
use keyring::Entry;
use secp256k1::{Secp256k1, SecretKey, PublicKey};
use kaspa_addresses::Address;
use hex;
use kaspa_wrpc_client::prelude::RpcApi;
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding, Resolver};
use kaspa_wrpc_client::prelude::{NetworkId, NetworkType};
use tokio::fs;
use rand::thread_rng;

const DEV_KEY_FILE: &str = ".kdapp-wallet-dev-key";

pub async fn create_wallet(dev_mode: bool) -> Result<()> {
    println!("Generating new wallet...");

    let secp = Secp256k1::new();
    let (secret_key, public_key) = secp.generate_keypair(&mut thread_rng());
    let private_key_hex = hex::encode(secret_key.secret_bytes());

    if dev_mode {
        println!("\nWARNING: Development mode enabled. Private key will be stored INSECURELY in a local file.\nDO NOT USE FOR REAL FUNDS!\n");
        fs::write(DEV_KEY_FILE, &private_key_hex).await?;
        println!("Wallet created and private key stored in '{}'.", DEV_KEY_FILE);

        let address = Address::new(
            kaspa_addresses::Prefix::Testnet, // Assuming Testnet for now, can be configurable
            kaspa_addresses::Version::PubKey,
            &public_key.serialize()[1..] // Remove compression byte for address
        );
        println!("\nWALLET NEEDS FUNDING! Visit https://faucet.kaspanet.io/ and fund: {}", address.to_string());

    } else {
        let service = "kdapp-wallet";
        let username = "default_wallet";

        let entry = Entry::new(service, username)?;
        entry.set_password(&private_key_hex)?;
        println!("Wallet created and stored securely in OS keychain.");
    }
    Ok(())
}

async fn get_private_key(dev_mode: bool) -> Result<String> {
    if dev_mode {
        if !fs::metadata(DEV_KEY_FILE).await.is_ok() {
            return Err(anyhow!("Development key file '{}' not found. Please create a wallet in dev mode first.", DEV_KEY_FILE));
        }
        Ok(fs::read_to_string(DEV_KEY_FILE).await?)
    } else {
        let service = "kdapp-wallet";
        let username = "default_wallet";
        let entry = Entry::new(service, username)?;
        Ok(entry.get_password()?)
    }
}

pub async fn get_address(dev_mode: bool) -> Result<()> {
    println!("Retrieving wallet address...");

    let private_key_hex = get_private_key(dev_mode).await?;

    let private_key_bytes = hex::decode(&private_key_hex)?;
    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&private_key_bytes)
        .map_err(|e| anyhow!("Failed to deserialize private key: {}", e))?;
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);

    let address = Address::new(
        kaspa_addresses::Prefix::Testnet, // Assuming Testnet for now, can be configurable
        kaspa_addresses::Version::PubKey,
        &public_key.serialize()[1..] // Remove compression byte for address
    );

    println!("Wallet Address: {}", address.to_string());

    Ok(())
}

pub async fn get_balance(rpc_url: Option<String>, dev_mode: bool) -> Result<()> {
    println!("Getting wallet balance...");

    let private_key_hex = get_private_key(dev_mode).await?;

    let private_key_bytes = hex::decode(&private_key_hex)?;
    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&private_key_bytes)
        .map_err(|e| anyhow!("Failed to deserialize private key: {}", e))?;
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);

    let address = Address::new(
        kaspa_addresses::Prefix::Testnet, // Assuming Testnet for now, can be configurable
        kaspa_addresses::Version::PubKey,
        &public_key.serialize()[1..] // Remove compression byte for address
    );

    // Connect to a Kaspa node
    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10); // Explicitly use Testnet with suffix 10

    let (resolver, url) = if let Some(url_str) = rpc_url {
        (None, Some(url_str))
    } else {
        (Some(Resolver::default()), None)
    };

    let rpc_client = KaspaRpcClient::new(
        WrpcEncoding::Borsh,
        url.as_deref(), // Apply .as_deref() here
        resolver,
        Some(network_id),
        None, // No connection timeout
    )?;

    println!("Attempting to connect to Kaspa node...");
    rpc_client.connect(Some(kaspa_wrpc_client::client::ConnectOptions::non_blocking_retry())).await?;
    println!("Successfully connected to Kaspa node!");

    let entries = rpc_client.get_utxos_by_addresses(vec![address]).await?;

    let mut total_balance = 0;
    for entry in entries {
        total_balance += entry.utxo_entry.amount;
    }

    println!("Wallet Balance: {} KAS", total_balance as f64 / 100_000_000.0); // Convert sompim to KAS

    Ok(())
}
