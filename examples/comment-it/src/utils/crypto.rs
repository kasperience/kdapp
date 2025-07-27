// src/utils/crypto.rs - Crypto utilities extracted from main.rs
use std::error::Error;
use secp256k1::{Secp256k1, SecretKey, Keypair};

/// Parse a private key from hex string
pub fn parse_private_key(hex_str: &str) -> Result<Keypair, Box<dyn Error>> {
    let secp = Secp256k1::new();
    let secret_bytes = hex::decode(hex_str)?;
    let secret_key = SecretKey::from_slice(&secret_bytes)?;
    Ok(Keypair::from_secret_key(&secp, &secret_key))
}

/// Generate a random keypair for development
pub fn generate_random_keypair() -> Keypair {
    let secp = Secp256k1::new();
    let secret_key = SecretKey::new(&mut rand::thread_rng());
    Keypair::from_secret_key(&secp, &secret_key)
}

/// Load private key from file (secure alternative to command line)
pub fn load_private_key_from_file(path: &str) -> Result<Keypair, Box<dyn Error>> {
    use std::fs;
    let key_hex = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read keyfile {}: {}", path, e))?
        .trim()
        .to_string();
    parse_private_key(&key_hex)
}