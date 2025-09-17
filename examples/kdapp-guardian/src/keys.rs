use std::borrow::Cow;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use log::warn;
use rand::thread_rng;
use secp256k1::{SecretKey, Secp256k1};
use thiserror::Error;

/// Errors that can occur when loading guardian signing keys.
#[derive(Debug, Error)]
pub enum KeyStorageError {
    #[error("I/O error while handling guardian key: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid guardian private key bytes")]
    InvalidSecretKey,
    #[error("key material was not valid hex: {0}")]
    InvalidEncoding(#[from] hex::FromHexError),
    #[error("HSM key slot `{slot}` not available in environment")]
    HsmUnavailable { slot: String },
}

/// Abstraction over how guardian signing keys are stored and retrieved.
pub trait GuardianKeyStorage {
    /// Load the guardian private key, creating it if the backend allows doing so.
    fn load_key(&self) -> Result<SecretKey, KeyStorageError>;
}

/// File-backed guardian key storage. The key is stored on disk and created on demand.
#[derive(Clone, Debug)]
pub struct FileKeyStorage {
    path: PathBuf,
}

impl FileKeyStorage {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn generate_and_store(&self) -> Result<SecretKey, KeyStorageError> {
        let secp = Secp256k1::new();
        let mut rng = thread_rng();
        let (sk, _) = secp.generate_keypair(&mut rng);

        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        fs::write(&self.path, sk.secret_bytes())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = fs::metadata(&self.path)?.permissions();
            perm.set_mode(0o600);
            fs::set_permissions(&self.path, perm)?;
        }

        Ok(sk)
    }
}

impl GuardianKeyStorage for FileKeyStorage {
    fn load_key(&self) -> Result<SecretKey, KeyStorageError> {
        match fs::read(&self.path) {
            Ok(bytes) => match SecretKey::from_slice(&bytes) {
                Ok(sk) => return Ok(sk),
                Err(_) => warn!(
                    "guardian: key at {} was invalid, generating a new one",
                    self.path.display()
                ),
            },
            Err(err) => {
                if err.kind() != ErrorKind::NotFound {
                    return Err(err.into());
                }
            }
        }

        self.generate_and_store()
    }
}

/// HSM-backed guardian key storage. The secret key is fetched from an environment variable
/// that represents a handle or export provided by the HSM driver.
#[derive(Clone, Debug)]
pub struct HsmKeyStorage {
    slot: String,
}

impl HsmKeyStorage {
    pub fn new(slot: impl Into<String>) -> Self {
        Self { slot: slot.into() }
    }

    pub fn slot(&self) -> &str {
        self.slot.as_str()
    }

    fn env_var(&self) -> String {
        let trimmed = self.slot.trim();
        if trimmed.is_empty() {
            "GUARDIAN_HSM_KEY".to_string()
        } else if let Some(rest) = trimmed.strip_prefix("env:") {
            rest.trim().to_string()
        } else {
            trimmed.to_string()
        }
    }
}

impl GuardianKeyStorage for HsmKeyStorage {
    fn load_key(&self) -> Result<SecretKey, KeyStorageError> {
        let env_var = self.env_var();
        let value = env::var(&env_var)
            .map_err(|_| KeyStorageError::HsmUnavailable { slot: env_var.clone() })?;
        let material = value.trim();
        let bytes = hex::decode(material)?;
        SecretKey::from_slice(&bytes).map_err(|_| KeyStorageError::InvalidSecretKey)
    }
}

/// Supported guardian key sources.
#[derive(Clone, Debug)]
pub enum GuardianKeySource {
    File(FileKeyStorage),
    Hsm(HsmKeyStorage),
}

impl GuardianKeySource {
    pub fn from_uri(uri: &str) -> Self {
        let trimmed = uri.trim();
        if let Some(rest) = trimmed.strip_prefix("hsm://") {
            let slot = rest.trim_start_matches('/');
            GuardianKeySource::Hsm(HsmKeyStorage::new(slot))
        } else if let Some(rest) = trimmed.strip_prefix("hsm:") {
            GuardianKeySource::Hsm(HsmKeyStorage::new(rest))
        } else {
            GuardianKeySource::File(FileKeyStorage::new(trimmed))
        }
    }

    pub fn describe(&self) -> Cow<'_, str> {
        match self {
            GuardianKeySource::File(store) => Cow::Owned(format!("file {}", store.path().display())),
            GuardianKeySource::Hsm(store) => {
                Cow::Owned(format!("HSM slot {}", store.env_var()))
            }
        }
    }
}

impl GuardianKeyStorage for GuardianKeySource {
    fn load_key(&self) -> Result<SecretKey, KeyStorageError> {
        match self {
            GuardianKeySource::File(store) => store.load_key(),
            GuardianKeySource::Hsm(store) => store.load_key(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hsm_uri() {
        let source = GuardianKeySource::from_uri("hsm://ledger");
        match source {
            GuardianKeySource::Hsm(store) => assert_eq!(store.env_var(), "ledger"),
            _ => panic!("expected HSM source"),
        }

        let env_source = GuardianKeySource::from_uri("hsm:env:CUSTOM_KEY");
        match env_source {
            GuardianKeySource::Hsm(store) => assert_eq!(store.env_var(), "CUSTOM_KEY"),
            _ => panic!("expected HSM source"),
        }
    }
}
