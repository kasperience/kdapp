// src/utils/keychain.rs - OS Keychain Integration for Kaspa Auth
use std::fs;
use keyring::Entry;
use secp256k1::{Secp256k1, SecretKey, Keypair};
use rand::rngs::OsRng;
use crate::wallet::{KaspaAuthWallet, WalletConfig};

pub struct KeychainConfig {
    pub service: String,
    pub dev_mode: bool,
}

impl Default for KeychainConfig {
    fn default() -> Self {
        KeychainConfig {
            service: "kaspa-auth".to_string(),
            dev_mode: false,
        }
    }
}

impl KeychainConfig {
    pub fn new(service: &str, dev_mode: bool) -> Self {
        KeychainConfig {
            service: service.to_string(),
            dev_mode,
        }
    }
}

pub struct KeychainManager {
    config: KeychainConfig,
    data_dir: String,
}

impl KeychainManager {
    pub fn new(config: KeychainConfig, data_dir: &str) -> Self {
        println!("DEBUG: KeychainManager::new - dev_mode: {}", config.dev_mode);
        KeychainManager { config, data_dir: data_dir.to_string() }
    }

    /// Create new wallet and store in OS keychain
    pub fn create_wallet(&self, username: &str) -> Result<KaspaAuthWallet, Box<dyn std::error::Error>> {
        println!("🔐 Generating new wallet and storing in OS keychain...");
        
        // Generate new keypair using real crypto
        let secp = Secp256k1::new();
        let (secret_key, _) = secp.generate_keypair(&mut OsRng);
        let private_key_hex = hex::encode(secret_key.secret_bytes());
        
        if self.config.dev_mode {
            println!("\n⚠️  WARNING: Development mode enabled.");
            println!("   Private key will be stored INSECURELY in local file.");
            println!("   DO NOT USE FOR REAL FUNDS!\n");
            
            // Create .kaspa-auth directory if it doesn't exist
            let wallet_dir = std::path::Path::new(&self.data_dir).join(".kaspa-auth");
            println!("DEBUG: Attempting to create directory: {:?}", wallet_dir);
            std::fs::create_dir_all(&wallet_dir)?;
            println!("DEBUG: Directory created: {:?}", wallet_dir);
            let dev_key_file = wallet_dir.join(format!("{}.key", username));
            println!("DEBUG: Attempting to write key to file: {:?}", dev_key_file);
            std::fs::write(&dev_key_file, &private_key_hex)?;
            println!("DEBUG: Key written to file: {:?}", dev_key_file);
            println!(" Wallet created and private key stored in '{}'.", dev_key_file.display());
        } else {
            // Store in OS keychain securely
            let entry = Entry::new(&self.config.service, username)?;
            entry.set_password(&private_key_hex)?;
            println!("🔐 Wallet created and stored securely in OS keychain.");
        }
        
        // Create KaspaAuthWallet from the generated keypair
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let wallet_config = WalletConfig::new(&self.data_dir);
        let wallet = KaspaAuthWallet {
            keypair,
            config: wallet_config,
            was_created: true,
        };
        
        // Display wallet info
        let kaspa_address = wallet.get_kaspa_address();
        println!("🔑 Public Key: {}", wallet.get_public_key_hex());
        println!("💰 Kaspa Address: {}", kaspa_address);
        println!("💡 Fund this address at: https://faucet.kaspanet.io/");
        println!("✅ Keychain wallet setup complete!\n");
        
        Ok(wallet)
    }

    /// Create new wallet and save it to the filesystem.
    pub fn create_and_save_wallet(&self, username: &str) -> Result<KaspaAuthWallet, Box<dyn std::error::Error>> {
        let mut config = WalletConfig::new(&self.data_dir);
        config.keypair_file = config.wallet_dir.join(format!("{}.key", username));

        // Generate new keypair
        use secp256k1::{Secp256k1, SecretKey};
        use rand::rngs::OsRng;
        let secp = Secp256k1::new();
        let (secret_key, _) = secp.generate_keypair(&mut OsRng);
        let keypair = Keypair::from_secret_key(&secp, &secret_key);

        // Save the secret key
        fs::create_dir_all(&config.wallet_dir)?;
        fs::write(&config.keypair_file, secret_key.as_ref())?;

        Ok(KaspaAuthWallet {
            keypair,
            config,
            was_created: true,
        })
    }
    
    /// Load existing wallet from OS keychain
    pub fn load_wallet(&self, username: &str) -> Result<KaspaAuthWallet, Box<dyn std::error::Error>> {
        println!("🔐 Loading wallet from OS keychain...");
        
        let private_key_hex = if self.config.dev_mode {
            let dev_key_file = format!(".kaspa-auth/{}.key", username);
            if !std::path::Path::new(&dev_key_file).exists() {
                return Err(format!("Development key file '{}' not found. Please create a wallet in dev mode first.", dev_key_file).into());
            }
            std::fs::read_to_string(&dev_key_file)?
        } else {
            let entry = Entry::new(&self.config.service, username)?;
            entry.get_password()?
        };
        
        // Recreate keypair from stored private key
        let secp = Secp256k1::new();
        let private_key_bytes = hex::decode(&private_key_hex)?;
        let secret_key = SecretKey::from_slice(&private_key_bytes)?;
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        
        let wallet_config = WalletConfig::new(&self.data_dir);
        let wallet = KaspaAuthWallet {
            keypair,
            config: wallet_config,
            was_created: false,
        };
        
        // Display wallet info
        let kaspa_address = wallet.get_kaspa_address();
        println!("✅ Wallet loaded from keychain");
        println!("🔑 Public Key: {}", wallet.get_public_key_hex());
        println!("💰 Kaspa Address: {}", kaspa_address);
        println!();
        
        Ok(wallet)
    }
    
    /// Load or create wallet with smooth UX
    pub fn load_or_create_wallet(&self, username: &str) -> Result<KaspaAuthWallet, Box<dyn std::error::Error>> {
        // Check if wallet already exists in keychain
        match self.load_wallet(username) {
            Ok(wallet) => {
                println!("🔄 REUSING existing keychain wallet for {}", username);
                Ok(wallet)
            },
            Err(_) => {
                println!("🆕 Creating NEW keychain wallet for {}", username);
                self.create_wallet(username)
            }
        }
    }
    
    /// Check if wallet exists in keychain
    pub fn wallet_exists(&self, username: &str) -> bool {
        if self.config.dev_mode {
            let dev_key_file = format!(".kaspa-auth/{}.key", username);
            std::path::Path::new(&dev_key_file).exists()
        } else {
            match Entry::new(&self.config.service, username) {
                Ok(entry) => entry.get_password().is_ok(),
                Err(_) => false,
            }
        }
    }
    
    /// Delete wallet from keychain
    pub fn delete_wallet(&self, username: &str) -> Result<(), Box<dyn std::error::Error>> {
        if self.config.dev_mode {
            let dev_key_file = format!(".kaspa-auth/{}.key", username);
            if std::path::Path::new(&dev_key_file).exists() {
                std::fs::remove_file(&dev_key_file)?;
                println!("🗑️  Deleted development wallet file: {}", dev_key_file);
            }
        } else {
            let entry = Entry::new(&self.config.service, username)?;
            entry.delete_credential()?;
            println!("🗑️  Deleted wallet from OS keychain: {}", username);
        }
        Ok(())
    }
    
    /// List available wallets (for development mode only)
    pub fn list_dev_wallets(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        if !self.config.dev_mode {
            return Err("Wallet listing only available in development mode".into());
        }
        
        let mut wallets = Vec::new();
        let _current_dir = std::env::current_dir()?;
        
        // Look in .kaspa-auth directory instead of current directory
        let kaspa_auth_dir = std::path::Path::new(".kaspa-auth");
        if kaspa_auth_dir.exists() {
            for entry in std::fs::read_dir(kaspa_auth_dir)? {
                let entry = entry?;
                let filename = entry.file_name().to_string_lossy().to_string();
                
                if filename.ends_with(".key") {
                    // Extract username from filename
                    let username = filename
                        .strip_suffix(".key")
                        .unwrap_or("unknown")
                        .to_string();
                    wallets.push(username);
                }
            }
        }
        
        Ok(wallets)
    }
}

/// Helper functions for easy integration with existing kaspa-auth code

/// Get wallet for command using keychain storage
pub fn get_keychain_wallet_for_command(command: &str, dev_mode: bool, data_dir: &str) -> Result<KaspaAuthWallet, Box<dyn std::error::Error>> {
    let keychain_config = KeychainConfig::new("kaspa-auth", dev_mode);
    let keychain_manager = KeychainManager::new(keychain_config, data_dir);
    
    // Map commands to keychain usernames
    let username = match command {
        "organizer-peer" | "http-peer" => "organizer-peer",
        "participant-peer" | "web-participant" | "authenticate" => "participant-peer", 
        _ => "default-wallet",
    };
    
    println!("🔐 Using OS keychain for {} wallet storage", username);
    keychain_manager.load_or_create_wallet(username)
}

/// Create specific wallet in keychain
pub fn create_keychain_wallet(username: &str, dev_mode: bool, data_dir: &str) -> Result<KaspaAuthWallet, Box<dyn std::error::Error>> {
    let keychain_config = KeychainConfig::new("kaspa-auth", dev_mode);
    let keychain_manager = KeychainManager::new(keychain_config, data_dir);
    keychain_manager.create_wallet(username)
}

/// Load specific wallet from keychain
pub fn load_keychain_wallet(username: &str, dev_mode: bool, data_dir: &str) -> Result<KaspaAuthWallet, Box<dyn std::error::Error>> {
    let keychain_config = KeychainConfig::new("kaspa-auth", dev_mode);
    let keychain_manager = KeychainManager::new(keychain_config, data_dir);
    keychain_manager.load_wallet(username)
}

/// Check if keychain wallet exists
pub fn keychain_wallet_exists(username: &str, dev_mode: bool, data_dir: &str) -> bool {
    let keychain_config = KeychainConfig::new("kaspa-auth", dev_mode);
    let keychain_manager = KeychainManager::new(keychain_config, data_dir);
    keychain_manager.wallet_exists(username)
}