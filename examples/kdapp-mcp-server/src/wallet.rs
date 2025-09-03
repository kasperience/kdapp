// src/wallet.rs - Wallet Management for kdapp MCP Server
use anyhow::Result;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use log::{info, warn};
use secp256k1::{Keypair, Secp256k1, SecretKey};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WalletConfig {
    pub wallet_dir: PathBuf,
    pub keypair_file: PathBuf,
    pub network_id: NetworkId,
}

impl Default for WalletConfig {
    fn default() -> Self {
        // Support custom wallet directory via environment variable
        let wallet_dir = env::var("KDAPP_WALLET_DIR")
            .map(|dir| Path::new(&dir).to_path_buf())
            .unwrap_or_else(|_| Path::new("agent_keys").to_path_buf());

        let keypair_file = wallet_dir.join("agent-wallet.key");
        let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);

        Self { wallet_dir, keypair_file, network_id }
    }
}

#[derive(Debug, Clone)]
pub struct AgentWallet {
    pub keypair: Keypair,
    pub config: WalletConfig,
    pub was_created: bool, // True if wallet was created this session
}

impl AgentWallet {
    /// Load existing wallet or create new one
    #[allow(dead_code)]
    pub fn load_or_create() -> Result<Self> {
        let config = WalletConfig::default();
        Self::load_or_create_with_config(config)
    }

    /// Load wallet for specific agent role
    pub fn load_or_create_for_agent(agent_name: &str) -> Result<Self> {
        let mut config = WalletConfig::default();

        // Use separate wallet files for different agents
        config.keypair_file = config.wallet_dir.join(format!("{agent_name}-wallet.key"));

        info!("📁 Loading {} wallet from: {}", agent_name, config.keypair_file.display());
        Self::load_or_create_with_config(config)
    }

    /// Load existing wallet or create new one with custom config
    pub fn load_or_create_with_config(config: WalletConfig) -> Result<Self> {
        // Check if this is first run
        let is_first_run = !config.keypair_file.exists();

        if is_first_run {
            Self::create_new_wallet(config)
        } else {
            Self::load_existing_wallet(config)
        }
    }

    /// Create new wallet
    fn create_new_wallet(config: WalletConfig) -> Result<Self> {
        info!("🎉 Creating new wallet for kdapp MCP Server!");
        info!("📁 Setting up wallet directory: {}", config.wallet_dir.display());

        // Create wallet directory
        fs::create_dir_all(&config.wallet_dir)?;

        info!("🔑 Generating secure keypair...");

        // Generate new keypair
        use rand::rngs::OsRng;
        let secp = Secp256k1::new();
        let (secret_key, _) = secp.generate_keypair(&mut OsRng);
        let keypair = Keypair::from_secret_key(&secp, &secret_key);

        // Save the secret key
        fs::write(&config.keypair_file, secret_key.as_ref())?;

        // Generate Kaspa address
        let network_prefix = Prefix::from(config.network_id);
        let kaspa_address = Address::new(network_prefix, Version::PubKey, &keypair.public_key().serialize()[1..]);

        info!("💾 Wallet saved to: {}", config.keypair_file.display());
        info!("🔑 Public Key: {}", hex::encode(keypair.public_key().serialize()));
        info!("💰 Funding Address: {kaspa_address}");
        info!("🌐 Network: {}", config.network_id);
        info!("💡 Fund this address at: https://faucet.kaspanet.io/");
        info!("✅ Wallet setup complete!");

        Ok(Self { keypair, config, was_created: true })
    }

    /// Load existing wallet
    fn load_existing_wallet(config: WalletConfig) -> Result<Self> {
        info!("📁 Loading wallet from: {}", config.keypair_file.display());

        // Load existing keypair
        let key_data = fs::read(&config.keypair_file)?;
        if key_data.len() != 32 {
            return Err(anyhow::anyhow!("Invalid wallet file format"));
        }

        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&key_data)?;
        let keypair = Keypair::from_secret_key(&secp, &secret_key);

        // Generate Kaspa address for display
        let network_prefix = Prefix::from(config.network_id);
        let kaspa_address = Address::new(network_prefix, Version::PubKey, &keypair.public_key().serialize()[1..]);

        info!("✅ Wallet loaded successfully");
        info!("🔑 Public Key: {}", hex::encode(keypair.public_key().serialize()));
        info!("💰 Funding Address: {kaspa_address}");
        info!("🌐 Network: {}", config.network_id);

        Ok(Self { keypair, config, was_created: false })
    }

    /// Get the Kaspa address for this wallet
    pub fn get_kaspa_address(&self) -> Address {
        let network_prefix = Prefix::from(self.config.network_id);
        Address::new(network_prefix, Version::PubKey, &self.keypair.public_key().serialize()[1..])
    }

    /// Get public key as hex string
    #[allow(dead_code)]
    pub fn get_public_key_hex(&self) -> String {
        hex::encode(self.keypair.public_key().serialize())
    }

    /// Check if wallet needs funding
    pub fn check_funding_status(&self) -> bool {
        // Currently suggests funding for newly created wallets
        // Real implementation would query UTXO set via Kaspa RPC
        self.was_created
    }

    /// Display funding reminder
    pub fn show_funding_reminder(&self) {
        if self.check_funding_status() {
            warn!("💡 REMINDER: Fund your address to enable on-chain transactions:");
            warn!("   Address: {}", self.get_kaspa_address());
            warn!("   Faucet: https://faucet.kaspanet.io/");
        }
    }
}
