use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_wrpc_client::prelude::*;
use secp256k1::Keypair;
use std::collections::HashMap;
use log::*;

/// Real UTXO Locking Manager for Economic Episode Contracts
#[derive(Debug, Clone)]
pub struct UtxoLockManager {
    // Track all UTXOs by address
    pub available_utxos: Vec<(TransactionOutpoint, UtxoEntry)>,
    pub locked_utxos: HashMap<u64, LockedUtxo>, // comment_id -> locked UTXO
    pub total_available_balance: u64,
    pub total_locked_balance: u64,
    pub kaspa_address: Address,
}

/// Information about a locked UTXO for a specific comment bond
#[derive(Debug, Clone)]
pub struct LockedUtxo {
    pub outpoint: TransactionOutpoint,
    pub entry: UtxoEntry,
    pub comment_id: u64,
    pub bond_amount: u64,
    pub lock_time: u64,
    pub unlock_conditions: UnlockCondition,
}

/// Conditions under which a locked UTXO can be unlocked
#[derive(Debug, Clone)]
pub enum UnlockCondition {
    /// Unlock after specified time with no disputes
    TimeBasedRelease { unlock_time: u64 },
    /// Unlock based on community vote outcome
    CommunityVote { vote_id: u64, required_consensus: f64 },
    /// Unlock via moderator multi-sig decision
    ModeratorDecision { required_signatures: u8, dispute_id: u64 },
    /// Automatic penalty - funds go to penalty pool
    Forfeited { violation_type: String },
}

impl UtxoLockManager {
    /// Create new UTXO manager with current wallet state
    pub async fn new(
        kaspad: &KaspaRpcClient,
        kaspa_address: Address,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        info!("🔍 Scanning wallet UTXOs for balance calculation...");
        
        let entries = kaspad.get_utxos_by_addresses(vec![kaspa_address.clone()]).await?;
        
        let available_utxos: Vec<(TransactionOutpoint, UtxoEntry)> = entries
            .into_iter()
            .map(|entry| {
                (
                    TransactionOutpoint::from(entry.outpoint),
                    UtxoEntry::from(entry.utxo_entry),
                )
            })
            .collect();
        
        let total_available_balance: u64 = available_utxos
            .iter()
            .map(|(_, entry)| entry.amount)
            .sum();
        
        info!("💰 Total available balance: {:.6} KAS", total_available_balance as f64 / 100_000_000.0);
        
        Ok(UtxoLockManager {
            available_utxos,
            locked_utxos: HashMap::new(),
            total_available_balance,
            total_locked_balance: 0,
            kaspa_address,
        })
    }
    
    /// Check if user has sufficient unlocked balance for a bond
    pub fn can_afford_bond(&self, bond_amount: u64) -> bool {
        let available_balance = self.total_available_balance - self.total_locked_balance;
        available_balance >= bond_amount
    }
    
    /// Get current unlocked balance
    pub fn get_available_balance(&self) -> u64 {
        self.total_available_balance - self.total_locked_balance
    }
    
    /// Lock a UTXO for a comment bond
    pub fn lock_utxo_for_comment(
        &mut self,
        comment_id: u64,
        bond_amount: u64,
        lock_duration_seconds: u64,
    ) -> Result<String, String> {
        // Check if we have sufficient balance
        if !self.can_afford_bond(bond_amount) {
            return Err(format!(
                "Insufficient unlocked balance. Available: {:.6} KAS, Required: {:.6} KAS",
                self.get_available_balance() as f64 / 100_000_000.0,
                bond_amount as f64 / 100_000_000.0
            ));
        }
        
        // Find a suitable UTXO (simplified - just use the first available one)
        if let Some((outpoint, entry)) = self.available_utxos.first().cloned() {
            if entry.amount >= bond_amount {
                let current_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                
                let unlock_time = current_time + lock_duration_seconds;
                
                let locked_utxo = LockedUtxo {
                    outpoint: outpoint.clone(),
                    entry: entry.clone(),
                    comment_id,
                    bond_amount,
                    lock_time: current_time,
                    unlock_conditions: UnlockCondition::TimeBasedRelease { unlock_time },
                };
                
                // Lock the UTXO
                self.locked_utxos.insert(comment_id, locked_utxo);
                self.total_locked_balance += bond_amount;
                
                // Remove from available (simplified - in reality, we'd track more precisely)
                // For now, we conceptually "reserve" this amount
                
                let utxo_ref = format!("{}:{}", outpoint.transaction_id, outpoint.index);
                info!("🔒 Locked {:.6} KAS for comment {} (UTXO: {})", 
                      bond_amount as f64 / 100_000_000.0, comment_id, utxo_ref);
                
                Ok(utxo_ref)
            } else {
                Err("No UTXO large enough for bond amount".to_string())
            }
        } else {
            Err("No UTXOs available".to_string())
        }
    }
    
    /// Check if a comment's bond can be unlocked
    pub fn can_unlock_bond(&self, comment_id: u64) -> bool {
        if let Some(locked_utxo) = self.locked_utxos.get(&comment_id) {
            match &locked_utxo.unlock_conditions {
                UnlockCondition::TimeBasedRelease { unlock_time } => {
                    let current_time = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    current_time >= *unlock_time
                }
                UnlockCondition::CommunityVote { .. } => {
                    // TODO: Check vote results
                    false
                }
                UnlockCondition::ModeratorDecision { .. } => {
                    // TODO: Check moderator signatures
                    false
                }
                UnlockCondition::Forfeited { .. } => {
                    // Already forfeited - cannot unlock
                    false
                }
            }
        } else {
            false
        }
    }
    
    /// Unlock a comment bond and return funds to available balance
    pub fn unlock_bond(&mut self, comment_id: u64) -> Result<u64, String> {
        if !self.can_unlock_bond(comment_id) {
            return Err("Bond cannot be unlocked yet".to_string());
        }
        
        if let Some(locked_utxo) = self.locked_utxos.remove(&comment_id) {
            self.total_locked_balance -= locked_utxo.bond_amount;
            
            info!("🔓 Unlocked {:.6} KAS bond for comment {}", 
                  locked_utxo.bond_amount as f64 / 100_000_000.0, comment_id);
            
            Ok(locked_utxo.bond_amount)
        } else {
            Err("No locked bond found for comment".to_string())
        }
    }
    
    /// Forfeit a bond to the penalty pool (due to violation)
    pub fn forfeit_bond(&mut self, comment_id: u64, violation_type: String) -> Result<u64, String> {
        if let Some(mut locked_utxo) = self.locked_utxos.remove(&comment_id) {
            locked_utxo.unlock_conditions = UnlockCondition::Forfeited { violation_type: violation_type.clone() };
            
            // Bond amount goes to penalty pool, not back to user
            let forfeited_amount = locked_utxo.bond_amount;
            self.total_locked_balance -= forfeited_amount;
            
            warn!("⚖️ Forfeited {:.6} KAS bond for comment {} (Violation: {})", 
                  forfeited_amount as f64 / 100_000_000.0, comment_id, violation_type);
            
            Ok(forfeited_amount)
        } else {
            Err("No locked bond found for comment".to_string())
        }
    }
    
    /// Get detailed balance information
    pub fn get_balance_info(&self) -> WalletBalanceInfo {
        WalletBalanceInfo {
            total_balance: self.total_available_balance,
            locked_balance: self.total_locked_balance,
            available_balance: self.get_available_balance(),
            locked_bonds_count: self.locked_utxos.len(),
            address: self.kaspa_address.clone(),
        }
    }
    
    /// Refresh UTXO state from blockchain
    pub async fn refresh_utxos(&mut self, kaspad: &KaspaRpcClient) -> Result<(), Box<dyn std::error::Error>> {
        info!("🔄 Refreshing UTXO state from blockchain...");
        
        let entries = kaspad.get_utxos_by_addresses(vec![self.kaspa_address.clone()]).await?;
        
        self.available_utxos = entries
            .into_iter()
            .map(|entry| {
                (
                    TransactionOutpoint::from(entry.outpoint),
                    UtxoEntry::from(entry.utxo_entry),
                )
            })
            .collect();
        
        let new_total = self.available_utxos
            .iter()
            .map(|(_, entry)| entry.amount)
            .sum();
        
        if new_total != self.total_available_balance {
            info!("💰 Balance updated: {:.6} KAS -> {:.6} KAS", 
                  self.total_available_balance as f64 / 100_000_000.0,
                  new_total as f64 / 100_000_000.0);
            self.total_available_balance = new_total;
        }
        
        Ok(())
    }
}

/// Detailed wallet balance information
#[derive(Debug, Clone)]
pub struct WalletBalanceInfo {
    pub total_balance: u64,
    pub locked_balance: u64,
    pub available_balance: u64,
    pub locked_bonds_count: usize,
    pub address: Address,
}

impl WalletBalanceInfo {
    pub fn display(&self) {
        println!("=== 💰 Wallet Balance ===");
        println!("📍 Address: {}", self.address);
        println!("💎 Total Balance: {:.6} KAS", self.total_balance as f64 / 100_000_000.0);
        println!("🔒 Locked in Bonds: {:.6} KAS", self.locked_balance as f64 / 100_000_000.0);
        println!("✅ Available: {:.6} KAS", self.available_balance as f64 / 100_000_000.0);
        println!("📊 Active Bonds: {}", self.locked_bonds_count);
        println!("========================");
    }
}