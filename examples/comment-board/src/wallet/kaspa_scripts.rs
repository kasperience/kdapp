use kaspa_consensus_core::{
    script::{ScriptBuilder, opcodes::*},
    tx::{Script, ScriptPublicKey},
};
use kaspa_addresses::{Address, AddressT};
use secp256k1::PublicKey;
use log::*;

/// Phase 2.0: Kaspa Script Generation for True UTXO Locking
/// 
/// This module provides script-based locking mechanisms that enforce spending conditions
/// directly at the blockchain level, eliminating the need for application-layer trust.

/// Bond unlock conditions for Phase 2.0 script-based enforcement
#[derive(Debug, Clone)]
pub enum ScriptUnlockCondition {
    /// Time-based release: funds unlock after specified timestamp
    TimeLock {
        unlock_time: u64,
        user_pubkey: PublicKey,
    },
    /// Multi-signature escape hatch: moderators can release funds early
    ModeratorRelease {
        user_pubkey: PublicKey,
        moderator_pubkeys: Vec<PublicKey>,
        required_signatures: usize,
    },
    /// Combined: time-lock OR moderator consensus
    TimeOrModerator {
        unlock_time: u64,
        user_pubkey: PublicKey,
        moderator_pubkeys: Vec<PublicKey>,
        required_signatures: usize,
    },
}

/// Phase 2.0: Create script-based time-lock for bond UTXOs
pub fn create_bond_timelock_script(
    unlock_time: u64,
    user_pubkey: &PublicKey,
) -> Result<Script, Box<dyn std::error::Error>> {
    info!("🔒 Creating time-lock script: unlock_time={}, user_pubkey={}", unlock_time, user_pubkey);
    
    // Create script: <unlock_time> OP_CHECKLOCKTIMEVERIFY OP_DROP <user_pubkey> OP_CHECKSIG
    let script = ScriptBuilder::new()
        .add_i64(unlock_time as i64)?              // Push unlock timestamp
        .add_op(OP_CHECKLOCKTIMEVERIFY)?           // Verify current time >= unlock_time
        .add_op(OP_DROP)?                          // Remove timestamp from stack
        .add_data(&user_pubkey.serialize())?       // Push user's public key
        .add_op(OP_CHECKSIG)?                      // Verify user's signature
        .drain();
    
    info!("✅ Time-lock script created: {} bytes", script.len());
    Ok(script)
}

/// Phase 2.0: Create multi-signature script for moderator dispute resolution
pub fn create_moderator_multisig_script(
    user_pubkey: &PublicKey,
    moderator_pubkeys: &[PublicKey],
    required_signatures: usize,
) -> Result<Script, Box<dyn std::error::Error>> {
    info!("🛡️ Creating multi-sig script: user={}, moderators={}, required={}", 
          user_pubkey, moderator_pubkeys.len(), required_signatures);
    
    if required_signatures > moderator_pubkeys.len() {
        return Err("Required signatures cannot exceed number of moderators".into());
    }
    
    // Create M-of-N multi-signature script
    let mut builder = ScriptBuilder::new();
    
    // Push required signature count
    builder = builder.add_i64(required_signatures as i64)?;
    
    // Push all moderator public keys
    for moderator_pubkey in moderator_pubkeys {
        builder = builder.add_data(&moderator_pubkey.serialize())?;
    }
    
    // Push total number of keys and add OP_CHECKMULTISIG
    builder = builder
        .add_i64(moderator_pubkeys.len() as i64)?
        .add_op(OP_CHECKMULTISIG)?;
    
    let script = builder.drain();
    
    info!("✅ Multi-sig script created: {} bytes", script.len());
    Ok(script)
}

/// Phase 2.0: Create combined time-lock OR multi-signature script
pub fn create_combined_unlock_script(
    unlock_time: u64,
    user_pubkey: &PublicKey,
    moderator_pubkeys: &[PublicKey],
    required_signatures: usize,
) -> Result<Script, Box<dyn std::error::Error>> {
    info!("🔐 Creating combined unlock script: time_lock OR multi_sig");
    
    // Create script: IF <timelock_branch> ELSE <multisig_branch> ENDIF
    let timelock_script = create_bond_timelock_script(unlock_time, user_pubkey)?;
    let multisig_script = create_moderator_multisig_script(user_pubkey, moderator_pubkeys, required_signatures)?;
    
    let combined_script = ScriptBuilder::new()
        .add_op(OP_IF)?                           // Start conditional
        .add_script(&timelock_script)?            // Time-lock branch
        .add_op(OP_ELSE)?                         // Alternative branch
        .add_script(&multisig_script)?            // Multi-sig branch  
        .add_op(OP_ENDIF)?                        // End conditional
        .drain();
    
    info!("✅ Combined script created: {} bytes (timelock: {}, multisig: {})", 
          combined_script.len(), timelock_script.len(), multisig_script.len());
    
    Ok(combined_script)
}

/// Phase 2.0: Generate script public key for bond UTXO creation
pub fn create_bond_script_pubkey(
    condition: &ScriptUnlockCondition,
) -> Result<ScriptPublicKey, Box<dyn std::error::Error>> {
    let script = match condition {
        ScriptUnlockCondition::TimeLock { unlock_time, user_pubkey } => {
            create_bond_timelock_script(*unlock_time, user_pubkey)?
        }
        ScriptUnlockCondition::ModeratorRelease { user_pubkey, moderator_pubkeys, required_signatures } => {
            create_moderator_multisig_script(user_pubkey, moderator_pubkeys, *required_signatures)?
        }
        ScriptUnlockCondition::TimeOrModerator { 
            unlock_time, 
            user_pubkey, 
            moderator_pubkeys, 
            required_signatures 
        } => {
            create_combined_unlock_script(*unlock_time, user_pubkey, moderator_pubkeys, *required_signatures)?
        }
    };
    
    // Create script public key from the script
    let script_pubkey = ScriptPublicKey::new(0, script); // Version 0 for standard scripts
    
    info!("🔑 Script public key created for bond UTXO");
    Ok(script_pubkey)
}

/// Phase 2.0: Validate script conditions for bond creation
pub fn validate_script_conditions(
    condition: &ScriptUnlockCondition,
    current_time: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    match condition {
        ScriptUnlockCondition::TimeLock { unlock_time, .. } => {
            if *unlock_time <= current_time {
                return Err("Unlock time must be in the future".into());
            }
            if *unlock_time > current_time + (365 * 24 * 60 * 60) { // Max 1 year
                return Err("Unlock time cannot be more than 1 year in the future".into());
            }
        }
        ScriptUnlockCondition::ModeratorRelease { moderator_pubkeys, required_signatures, .. } => {
            if moderator_pubkeys.is_empty() {
                return Err("At least one moderator public key required".into());
            }
            if *required_signatures == 0 {
                return Err("At least one signature required".into());
            }
            if *required_signatures > moderator_pubkeys.len() {
                return Err("Required signatures cannot exceed number of moderators".into());
            }
        }
        ScriptUnlockCondition::TimeOrModerator { unlock_time, moderator_pubkeys, required_signatures, .. } => {
            // Validate both time-lock and multi-sig conditions
            if *unlock_time <= current_time {
                return Err("Unlock time must be in the future".into());
            }
            if moderator_pubkeys.is_empty() {
                return Err("At least one moderator public key required".into());
            }
            if *required_signatures > moderator_pubkeys.len() {
                return Err("Required signatures cannot exceed number of moderators".into());
            }
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Secp256k1, SecretKey};
    use rand::thread_rng;
    
    fn generate_test_keypair() -> (SecretKey, PublicKey) {
        let secp = Secp256k1::new();
        secp256k1::generate_keypair(&mut thread_rng())
    }
    
    #[test]
    fn test_timelock_script_creation() {
        let (_, user_pubkey) = generate_test_keypair();
        let unlock_time = 1000000000; // Future timestamp
        
        let script = create_bond_timelock_script(unlock_time, &user_pubkey).unwrap();
        assert!(!script.is_empty());
        assert!(script.len() > 10); // Should have meaningful content
    }
    
    #[test]
    fn test_multisig_script_creation() {
        let (_, user_pubkey) = generate_test_keypair();
        let (_, mod1_pubkey) = generate_test_keypair();
        let (_, mod2_pubkey) = generate_test_keypair();
        let moderator_pubkeys = vec![mod1_pubkey, mod2_pubkey];
        
        let script = create_moderator_multisig_script(&user_pubkey, &moderator_pubkeys, 2).unwrap();
        assert!(!script.is_empty());
    }
    
    #[test]
    fn test_combined_script_creation() {
        let (_, user_pubkey) = generate_test_keypair();
        let (_, mod1_pubkey) = generate_test_keypair();
        let moderator_pubkeys = vec![mod1_pubkey];
        let unlock_time = 2000000000; // Future timestamp
        
        let script = create_combined_unlock_script(unlock_time, &user_pubkey, &moderator_pubkeys, 1).unwrap();
        assert!(!script.is_empty());
    }
    
    #[test]
    fn test_script_condition_validation() {
        let (_, user_pubkey) = generate_test_keypair();
        let (_, mod_pubkey) = generate_test_keypair();
        let current_time = 1000000000;
        
        // Valid time-lock condition
        let timelock_condition = ScriptUnlockCondition::TimeLock {
            unlock_time: current_time + 3600, // 1 hour in future
            user_pubkey,
        };
        assert!(validate_script_conditions(&timelock_condition, current_time).is_ok());
        
        // Invalid time-lock condition (in the past)
        let invalid_timelock = ScriptUnlockCondition::TimeLock {
            unlock_time: current_time - 3600, // 1 hour in past
            user_pubkey,
        };
        assert!(validate_script_conditions(&invalid_timelock, current_time).is_err());
        
        // Valid moderator condition
        let moderator_condition = ScriptUnlockCondition::ModeratorRelease {
            user_pubkey,
            moderator_pubkeys: vec![mod_pubkey],
            required_signatures: 1,
        };
        assert!(validate_script_conditions(&moderator_condition, current_time).is_ok());
    }
}