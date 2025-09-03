// src/wallet.rs - Unified Wallet Management System
use crate::utils::keychain::{get_keychain_wallet_for_command, keychain_wallet_exists};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use secp256k1::Keypair;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WalletConfig {
    pub wallet_dir: PathBuf,
    pub keypair_file: PathBuf,
    pub network_id: NetworkId,
}

impl WalletConfig {
    pub fn new(data_dir: &str) -> Self {
        let wallet_dir = Path::new(data_dir).join(".kaspa-auth").to_path_buf();
        let keypair_file = wallet_dir.join("wallet.key");
        let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);

        Self { wallet_dir, keypair_file, network_id }
    }
}

// impl WalletConfig {
//     pub fn new(data_dir: &str) -> Self {
//         let wallet_dir = Path::new(data_dir).join(".kaspa-auth").to_path_buf();
//         let keypair_file = wallet_dir.join("wallet.key");
//         let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);

//         Self {
//             wallet_dir,
//             keypair_file,
//             network_id,
//         }
//     }
// }

impl Default for WalletConfig {
    fn default() -> Self {
        Self::new(".")
    }
}
#[derive(Debug, Clone)]
pub struct KaspaAuthWallet {
    pub keypair: Keypair,
    pub config: WalletConfig,
    pub was_created: bool, // True if wallet was created this session
}

impl KaspaAuthWallet {
    /// Load existing wallet or create new one with smooth UX
    pub fn load_or_create(data_dir: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config = WalletConfig::new(data_dir);
        Self::load_or_create_with_config(config)
    }

    /// Load wallet for specific role (server/client) with separate wallet files
    pub fn load_or_create_with_role(role: &str, data_dir: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut config = WalletConfig::new(data_dir);

        // Use separate wallet files for server vs client
        config.keypair_file = config.wallet_dir.join(format!("{role}-wallet.key"));

        println!("📁 Loading {} wallet from: {}", role, config.keypair_file.display());
        Self::load_or_create_with_config(config)
    }

    /// Load existing wallet or create new one with custom config
    pub fn load_or_create_with_config(config: WalletConfig) -> Result<Self, Box<dyn std::error::Error>> {
        // Check if this is first run
        let is_first_run = !config.keypair_file.exists();

        if is_first_run {
            Self::create_new_wallet_ux(config)
        } else {
            Self::load_existing_wallet_ux(config)
        }
    }

    /// Create new wallet with welcoming UX
    fn create_new_wallet_ux(config: WalletConfig) -> Result<Self, Box<dyn std::error::Error>> {
        println!("🎉 Welcome to Kaspa Authentication!");
        println!("📁 Setting up your wallet directory: {}", config.wallet_dir.display());

        // Create wallet directory
        fs::create_dir_all(&config.wallet_dir)?;

        println!("🔑 Generating secure keypair...");

        // Generate new keypair
        use rand::rngs::OsRng;
        use secp256k1::Secp256k1;
        let secp = Secp256k1::new();
        let (secret_key, _) = secp.generate_keypair(&mut OsRng);
        let keypair = Keypair::from_secret_key(&secp, &secret_key);

        // Save the secret key
        fs::write(&config.keypair_file, secret_key.as_ref())?;

        // Generate Kaspa address
        let network_prefix = Prefix::from(config.network_id);
        let kaspa_address = Address::new(network_prefix, Version::PubKey, &keypair.x_only_public_key().0.serialize());

        println!("💾 Wallet saved to: {}", config.keypair_file.display());
        println!("🔑 Public Key: {}", hex::encode(keypair.public_key().serialize()));
        println!("💰 Funding Address: {kaspa_address}");
        println!("🌐 Network: {}", config.network_id);
        println!("💡 Fund this address at: https://faucet.kaspanet.io/");
        println!("✅ Wallet setup complete!");
        println!();

        Ok(Self { keypair, config, was_created: true })
    }

    /// Load existing wallet with status UX
    fn load_existing_wallet_ux(config: WalletConfig) -> Result<Self, Box<dyn std::error::Error>> {
        println!("📁 Loading wallet from: {}", config.keypair_file.display());

        // Load existing keypair
        let key_data = fs::read(&config.keypair_file)?;
        if key_data.len() != 32 {
            return Err("Invalid wallet file format".into());
        }

        use secp256k1::{Secp256k1, SecretKey};
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&key_data)?;
        let keypair = Keypair::from_secret_key(&secp, &secret_key);

        // Generate Kaspa address for display
        let network_prefix = Prefix::from(config.network_id);
        let kaspa_address = Address::new(network_prefix, Version::PubKey, &keypair.x_only_public_key().0.serialize());

        println!("✅ Wallet loaded successfully");
        println!("🔑 Public Key: {}", hex::encode(keypair.public_key().serialize()));
        println!("💰 Funding Address: {kaspa_address}");
        println!("🌐 Network: {}", config.network_id);
        println!();

        Ok(Self { keypair, config, was_created: false })
    }

    /// Get the Kaspa address for this wallet
    pub fn get_kaspa_address(&self) -> Address {
        let network_prefix = Prefix::from(self.config.network_id);
        Address::new(network_prefix, Version::PubKey, &self.keypair.x_only_public_key().0.serialize())
    }

    /// Get public key as hex string
    pub fn get_public_key_hex(&self) -> String {
        hex::encode(self.keypair.public_key().serialize())
    }

    /// Get private key as hex string (for organizer-side signing)
    pub fn get_private_key_hex(&self) -> String {
        hex::encode(self.keypair.secret_key().as_ref())
    }

    /// Check if wallet needs funding - currently returns true for new wallets
    /// Future enhancement: integrate with Kaspa RPC to check actual balance
    pub fn check_funding_status(&self) -> bool {
        // Currently suggests funding for newly created wallets
        // Real implementation would query UTXO set via Kaspa RPC
        self.was_created
    }

    /// Display funding reminder
    pub fn show_funding_reminder(&self) {
        if self.check_funding_status() {
            println!("💡 REMINDER: Fund your address to test economic features:");
            println!("   Address: {}", self.get_kaspa_address());
            println!("   Faucet: https://faucet.kaspanet.io/");
            println!();
        }
    }

    /// Load wallet for specific command with appropriate messaging
    pub fn load_for_command(command: &str, data_dir: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Use separate wallet files for organizer vs participant peers
        let wallet = match command {
            "organizer-peer" | "http-peer" => Self::load_or_create_with_role("organizer-peer", data_dir)?,
            "participant-peer" | "web-participant" | "authenticate" => Self::load_or_create_with_role("participant-peer", data_dir)?,
            _ => Self::load_or_create(data_dir)?,
        };

        match command {
            "organizer-peer" | "http-peer" => {
                let kaspa_addr = wallet.get_kaspa_address();
                if wallet.was_created {
                    println!("🆕 Creating NEW organizer-peer wallet");
                    println!("🔑 New Kaspa address: {kaspa_addr}");
                    println!("💾 Wallet saved to: .kaspa-auth/organizer-peer-wallet.key");
                } else {
                    println!("🔄 REUSING existing organizer-peer wallet");
                    println!("🔑 Existing Kaspa address: {kaspa_addr}");
                    println!("📁 Loaded from: .kaspa-auth/organizer-peer-wallet.key");
                }
                wallet.show_funding_reminder();
            }
            "participant-peer" | "web-participant" | "authenticate" => {
                let kaspa_addr = wallet.get_kaspa_address();
                if wallet.was_created {
                    println!("🆕 Creating NEW participant-peer wallet");
                    println!("🔑 New Kaspa address: {kaspa_addr}");
                    println!("💾 Wallet saved to: .kaspa-auth/participant-peer-wallet.key");
                } else {
                    println!("🔄 REUSING existing participant-peer wallet");
                    println!("🔑 Existing Kaspa address: {kaspa_addr}");
                    println!("📁 Loaded from: .kaspa-auth/participant-peer-wallet.key");
                }
            }
            _ => {
                println!("🔑 Using {} wallet ({})", command, if wallet.was_created { "NEW" } else { "EXISTING" });
            }
        }

        Ok(wallet)
    }

    /// Create wallet from provided private key (for --key option)
    pub fn from_private_key(private_key_hex: &str, data_dir: &str) -> Result<Self, Box<dyn std::error::Error>> {
        use secp256k1::{Secp256k1, SecretKey};

        let secp = Secp256k1::new();
        let secret_bytes = hex::decode(private_key_hex)?;
        let secret_key = SecretKey::from_slice(&secret_bytes)?;
        let keypair = Keypair::from_secret_key(&secp, &secret_key);

        println!("🔑 Using provided private key");
        println!("🔑 Public Key: {}", hex::encode(keypair.public_key().serialize()));

        Ok(Self { keypair, config: WalletConfig::new(data_dir), was_created: false })
    }

    /// Load wallet using OS keychain instead of file system
    pub fn load_from_keychain(command: &str, dev_mode: bool, data_dir: &str) -> Result<Self, Box<dyn std::error::Error>> {
        println!("🔐 Loading wallet from OS keychain for command: {command}");
        get_keychain_wallet_for_command(command, dev_mode, data_dir)
    }

    /// Check if keychain wallet exists for command
    pub fn keychain_wallet_exists_for_command(command: &str, dev_mode: bool, data_dir: &str) -> bool {
        let username = match command {
            "organizer-peer" | "http-peer" => "organizer-peer",
            "participant-peer" | "web-participant" | "authenticate" => "participant-peer",
            _ => "default-wallet",
        };
        keychain_wallet_exists(username, dev_mode, data_dir)
    }
}

/// Get wallet for any command with unified UX
pub fn get_wallet_for_command(
    command: &str,
    private_key: Option<&str>,
    data_dir: &str,
) -> Result<KaspaAuthWallet, Box<dyn std::error::Error>> {
    match private_key {
        Some(key_hex) => {
            println!("🔑 Using provided private key for {command}");
            KaspaAuthWallet::from_private_key(key_hex, data_dir)
        }
        None => KaspaAuthWallet::load_for_command(command, data_dir),
    }
}

/// Get wallet with choice of storage method (file-based or keychain)
pub fn get_wallet_for_command_with_storage(
    command: &str,
    private_key: Option<&str>,
    use_keychain: bool,
    dev_mode: bool,
    data_dir: &str,
) -> Result<KaspaAuthWallet, Box<dyn std::error::Error>> {
    match private_key {
        Some(key_hex) => {
            println!("🔑 Using provided private key for {command}");
            KaspaAuthWallet::from_private_key(key_hex, data_dir)
        }
        None => {
            if use_keychain {
                println!("🔐 Using OS keychain for wallet storage");
                KaspaAuthWallet::load_from_keychain(command, dev_mode, data_dir)
            } else {
                println!("📁 Using file-based wallet storage");
                KaspaAuthWallet::load_for_command(command, data_dir)
            }
        }
    }
}
