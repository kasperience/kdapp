//! Public Key Infrastructure (PKI) methods and helpers.

use borsh::{BorshDeserialize, BorshSerialize};
use rand::rngs::OsRng;
use secp256k1::ecdsa::Signature;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PubKey(pub PublicKey);

impl std::fmt::Debug for PubKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for PubKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Ord for PubKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.serialize().cmp(&other.0.serialize())
    }
}

impl PartialOrd for PubKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for PubKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash the stable compressed representation
        state.write(&self.0.serialize());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sig(pub Signature);
impl BorshSerialize for PubKey {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(&self.0.serialize())
    }
}

impl BorshDeserialize for PubKey {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut buf = [0u8; 33]; // compressed pubkey
        reader.read_exact(&mut buf)?;
        let pk =
            PublicKey::from_slice(&buf).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid public key"))?;
        Ok(PubKey(pk))
    }
}

impl BorshSerialize for Sig {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // Serialize DER-encoded signature with a u32 length prefix (borsh-idiomatic)
        let der = self.0.serialize_der();
        // write length via borsh to remain consistent with other fields
        (der.len() as u32).serialize(writer)?;
        writer.write_all(&der)
    }
}

impl BorshDeserialize for Sig {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        // Read only the declared number of bytes so subsequent fields remain intact
        let len = u32::deserialize_reader(reader)? as usize;
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;
        let sig = Signature::from_der(&buf).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid signature"))?;
        Ok(Sig(sig))
    }
}

pub fn generate_keypair() -> (SecretKey, PubKey) {
    let secp = Secp256k1::new();
    let mut rng = OsRng;
    let secret_key = SecretKey::new(&mut rng);
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    (secret_key, PubKey(public_key))
}

/// Convert any serializable object into a `secp256k1::Message` by:
/// - serializing it with `bincode`
/// - hashing it with SHA-256
pub fn to_message<T: BorshSerialize>(object: &T) -> Message {
    let bytes = borsh::to_vec(object).expect("serialization failed");
    let hash = Sha256::digest(&bytes);
    Message::from_digest_slice(&hash).expect("hash must be 32 bytes")
}

/// Sign a message using a `SecretKey`
pub fn sign_message(secret_key: &SecretKey, message: &Message) -> Sig {
    let secp = Secp256k1::signing_only();
    Sig(secp.sign_ecdsa(message, secret_key))
}

pub fn verify_signature(public_key: &PubKey, message: &Message, signature: &Sig) -> bool {
    let secp = Secp256k1::verification_only();
    secp.verify_ecdsa(message, &signature.0, &public_key.0).is_ok()
}
