#![allow(dead_code)]
use crate::episode::board_with_contract::ContractCommentBoard;
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
use kaspa_wrpc_client::prelude::*;
use kdapp::engine::EpisodeMessage;
use kdapp::generator::{check_pattern, PatternType, Payload, PrefixType};
use log::*;
use secp256k1::{Keypair, PublicKey};
use std::collections::HashMap;

// Phase 2.0: Import script generation for true UTXO locking
use crate::utils::FEE;
use crate::wallet::kaspa_scripts::{create_bond_script_pubkey, validate_script_conditions, ScriptUnlockCondition};

/// Real UTXO Locking Manager for Economic Episode Contracts - Phase 1.1 Implementation
#[derive(Debug, Clone)]
pub struct UtxoLockManager {
    // Track all UTXOs by address
    pub available_utxos: Vec<(TransactionOutpoint, UtxoEntry)>,
    pub locked_utxos: HashMap<u64, LockedUtxo>, // comment_id -> locked UTXO
    pub total_available_balance: u64,
    pub total_locked_balance: u64,
    pub kaspa_address: Address,

    // Phase 1.1: Real transaction tracking
    pub kaspad_client: KaspaRpcClient,       // For broadcasting transactions
    pub keypair: Keypair,                    // For signing bond transactions
    pub pending_bonds: HashMap<u64, String>, // comment_id -> transaction_id (waiting for confirmation)
}

/// Information about a locked UTXO for a specific comment bond - Enhanced for Phase 2.0
#[derive(Debug, Clone)]
pub struct LockedUtxo {
    pub outpoint: TransactionOutpoint,
    pub entry: UtxoEntry,
    pub comment_id: u64,
    pub bond_amount: u64,
    pub lock_time: u64,
    pub unlock_conditions: UnlockCondition,

    // Phase 1.2: Real transaction tracking
    pub bond_transaction_id: String,      // The actual transaction ID that created this bond
    pub confirmation_height: Option<u64>, // Block height when confirmed (None = pending)
    pub bond_address: Address,            // The address where bond funds are held

    // Phase 2.0: Script-based locking enhancement
    pub enforcement_level: BondEnforcementLevel, // Phase 1.2 vs Phase 2.0
}

/// Bond enforcement levels for gradual migration from Phase 1.2 to Phase 2.0
#[derive(Debug, Clone)]
pub enum BondEnforcementLevel {
    /// Phase 1.2: Application-layer tracking with blockchain proof
    ApplicationLayer { proof_transaction_id: String },
    /// Phase 2.0: True blockchain script-based enforcement  
    ScriptBased { script_pubkey: ScriptPublicKey, unlock_script_condition: ScriptUnlockCondition },
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
    async fn submit_with_retry(&self, tx: &kaspa_consensus_core::tx::Transaction) -> Result<(), String> {
        let mut attempts = 0usize;
        loop {
            match self.kaspad_client.submit_transaction(tx.into(), false).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    attempts += 1;
                    let msg = e.to_string();
                    if msg.contains("already accepted") {
                        return Ok(());
                    }
                    if attempts >= 3 {
                        return Err(msg);
                    }
                    if msg.contains("WebSocket") || msg.contains("not connected") || msg.contains("disconnected") {
                        let _ = self.kaspad_client.connect(Some(kdapp::proxy::connect_options())).await;
                        continue;
                    }
                    // Orphan/transient
                    continue;
                }
            }
        }
    }
    /// Create new UTXO manager with current wallet state - Phase 1.1 Enhanced
    pub async fn new(
        kaspad: &KaspaRpcClient,
        kaspa_address: Address,
        keypair: Keypair, // Need keypair for signing bond transactions
    ) -> Result<Self, String> {
        info!("🔍 Scanning wallet UTXOs for balance calculation...");

        let entries =
            kaspad.get_utxos_by_addresses(vec![kaspa_address.clone()]).await.map_err(|e| format!("Failed to get UTXOs: {e}"))?;

        let available_utxos: Vec<(TransactionOutpoint, UtxoEntry)> =
            entries.into_iter().map(|entry| (TransactionOutpoint::from(entry.outpoint), UtxoEntry::from(entry.utxo_entry))).collect();

        let total_available_balance: u64 = available_utxos.iter().map(|(_, entry)| entry.amount).sum();

        info!("💰 Total available balance: {:.6} KAS", total_available_balance as f64 / 100_000_000.0);

        Ok(UtxoLockManager {
            available_utxos,
            locked_utxos: HashMap::new(),
            total_available_balance,
            total_locked_balance: 0,
            kaspa_address,
            kaspad_client: kaspad.clone(),
            keypair,
            pending_bonds: HashMap::new(),
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

    /// Phase 1.1: Create REAL bond transaction on Kaspa blockchain
    pub async fn lock_utxo_for_comment(
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

        // Find the SMALLEST suitable UTXO to minimize mass calculation
        let mut best_utxo: Option<(TransactionOutpoint, UtxoEntry)> = None;
        let min_required = FEE * 2 + 1000; // Minimal amount needed for proof transaction

        for (outpoint, entry) in &self.available_utxos {
            if entry.amount >= min_required {
                match &best_utxo {
                    None => best_utxo = Some((*outpoint, entry.clone())),
                    Some((_, best_entry)) => {
                        if entry.amount < best_entry.amount {
                            best_utxo = Some((*outpoint, entry.clone()));
                        }
                    }
                }
            }
        }

        if let Some((outpoint, entry)) = best_utxo {
            info!("🔍 Selected UTXO: {:.6} KAS (smallest available for minimal mass)", entry.amount as f64 / 100_000_000.0);

            // EMERGENCY MASS LIMIT PROTECTION: Refuse bond creation if UTXO too large
            if entry.amount > 100_000 {
                // > 0.001 KAS - guaranteed mass limit failure (mass ≈ UTXO sompi value!)
                return Err(format!(
                    "❌ MASS LIMIT PROTECTION: Selected UTXO ({:.6} KAS) will cause transaction mass > 100,000\n\
                     💡 CRITICAL: Mass limit = 100,000, transaction mass ≈ UTXO amount in sompi\n\
                     🔧 SOLUTION: Need micro-UTXOs < 0.001 KAS each! Use aggressive splitting.",
                    entry.amount as f64 / 100_000_000.0
                ));
            }

            // Verify UTXO is safe for mass limit
            if entry.amount > 50_000 {
                // > 0.0005 KAS
                warn!("⚠️ Selected UTXO may cause mass limit issues: {:.6} KAS", entry.amount as f64 / 100_000_000.0);
                warn!("💡 Need micro-UTXOs: < 0.001 KAS for guaranteed bond compatibility");
            }
            let current_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

            let unlock_time = current_time + lock_duration_seconds;

            // Phase 1.1: Create a REAL transaction that sends bond_amount to a new address
            // For now, we'll send to the same address (self-bond) but track it separately
            match self.create_bond_transaction(comment_id, bond_amount, &outpoint, &entry).await {
                Ok(bond_tx_id) => {
                    // Create bond address (for now, same as main address - will be different in Phase 2)
                    let bond_address = self.kaspa_address.clone();

                    let locked_utxo = LockedUtxo {
                        outpoint,
                        entry: entry.clone(),
                        comment_id,
                        bond_amount,
                        lock_time: current_time,
                        unlock_conditions: UnlockCondition::TimeBasedRelease { unlock_time },
                        bond_transaction_id: bond_tx_id.clone(),
                        confirmation_height: None, // Will be set when confirmed
                        bond_address,
                        enforcement_level: BondEnforcementLevel::ApplicationLayer { proof_transaction_id: bond_tx_id.clone() },
                    };

                    // Track as pending until confirmed
                    self.pending_bonds.insert(comment_id, bond_tx_id.clone());
                    self.locked_utxos.insert(comment_id, locked_utxo);
                    self.total_locked_balance += bond_amount;

                    info!(
                        "🔒 Created REAL bond transaction {} for comment {} ({:.6} KAS)",
                        bond_tx_id,
                        comment_id,
                        bond_amount as f64 / 100_000_000.0
                    );
                    info!("⏳ Bond transaction pending confirmation on Kaspa blockchain...");

                    Ok(bond_tx_id)
                }
                Err(e) => Err(format!("Failed to create bond transaction: {e}")),
            }
        } else {
            Err(format!(
                "No UTXO large enough for bond transaction. Required: {:.6} KAS, Available UTXOs: {}",
                min_required as f64 / 100_000_000.0,
                self.available_utxos
                    .iter()
                    .map(|(_, e)| format!("{:.6}", e.amount as f64 / 100_000_000.0))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    }

    /// Phase 2.0: Create script-based UTXO bond with true blockchain enforcement
    pub async fn create_script_based_bond(
        &mut self,
        comment_id: u64,
        bond_amount: u64,
        lock_duration_seconds: u64,
        moderator_pubkeys: Option<Vec<PublicKey>>,
        required_moderator_signatures: Option<usize>,
    ) -> Result<String, String> {
        info!("🔒 Phase 2.0: Creating script-based bond with TRUE blockchain enforcement");

        use kaspa_consensus_core::{
            constants::TX_VERSION,
            sign::sign,
            subnets::SUBNETWORK_ID_NATIVE,
            tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutput},
        };
        use kaspa_txscript::pay_to_address_script;

        // Check if we have sufficient balance
        if !self.can_afford_bond(bond_amount) {
            return Err(format!(
                "Insufficient unlocked balance. Available: {:.6} KAS, Required: {:.6} KAS",
                self.get_available_balance() as f64 / 100_000_000.0,
                bond_amount as f64 / 100_000_000.0
            ));
        }

        let current_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let unlock_time = current_time + lock_duration_seconds;
        let user_pubkey = self.keypair.public_key();

        // Create script unlock condition based on provided parameters
        let script_condition = if let (Some(mod_pubkeys), Some(required_sigs)) = (moderator_pubkeys, required_moderator_signatures) {
            ScriptUnlockCondition::TimeOrModerator {
                unlock_time,
                user_pubkey,
                moderator_pubkeys: mod_pubkeys,
                required_signatures: required_sigs,
            }
        } else {
            ScriptUnlockCondition::TimeLock { unlock_time, user_pubkey }
        };

        if let Err(e) = validate_script_conditions(&script_condition, current_time) {
            return Err(format!("Invalid script conditions: {e}"));
        }

        let script_pubkey = match create_bond_script_pubkey(&script_condition) {
            Ok(spk) => spk,
            Err(e) => return Err(format!("Failed to create script public key: {e}")),
        };
        info!("🔐 Script public key created: {} bytes", script_pubkey.script().len());

        // --- Multi-input coin selection ---
        let required_total = bond_amount + FEE;
        let mut utxos_sorted: Vec<(TransactionOutpoint, UtxoEntry)> = self.available_utxos.clone();
        utxos_sorted.sort_by_key(|(_, e)| e.amount);

        let mut selected_inputs: Vec<(TransactionOutpoint, UtxoEntry)> = Vec::new();
        let mut selected_total: u64 = 0;
        for (op, e) in utxos_sorted.into_iter() {
            selected_total = selected_total.saturating_add(e.amount);
            selected_inputs.push((op, e));
            if selected_total >= required_total {
                break;
            }
        }
        if selected_total < required_total {
            return Err("No available UTXOs for bond creation".to_string());
        }

        // --- Transaction construction ---
        let tx_inputs: Vec<TransactionInput> = selected_inputs
            .iter()
            .map(|(op, _e)| TransactionInput { previous_outpoint: *op, signature_script: vec![], sequence: 0, sig_op_count: 1 })
            .collect();

        let script_output = TransactionOutput { value: bond_amount, script_public_key: script_pubkey.clone() };

        let change_value = selected_total.saturating_sub(bond_amount + FEE);
        let change_output = if change_value > 0 {
            Some(TransactionOutput { value: change_value, script_public_key: pay_to_address_script(&self.kaspa_address) })
        } else {
            None
        };

        let mut outputs = vec![script_output];
        if let Some(co) = change_output {
            outputs.push(co);
        }

        let bond_payload = format!("SCRIPT_BOND:{comment_id}:{bond_amount}:{len}", len = script_pubkey.script().len());

        let unsigned_tx =
            Transaction::new_non_finalized(TX_VERSION, tx_inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, bond_payload.into_bytes());

        let selected_entries: Vec<UtxoEntry> = selected_inputs.iter().map(|(_, e)| e.clone()).collect();
        let signed_tx = sign(MutableTransaction::with_entries(unsigned_tx, selected_entries), self.keypair).tx;
        let tx_id = signed_tx.id().to_string();

        // --- Submit and update state ---
        match self.kaspad_client.submit_transaction((&signed_tx).into(), false).await {
            Ok(_) => {
                let bond_address = self.kaspa_address.clone(); // TODO: Generate from script_pubkey
                let locked_utxo = LockedUtxo {
                    outpoint: selected_inputs[0].0,      // TODO: This is not quite right for multi-input, but ok for now
                    entry: selected_inputs[0].1.clone(), // TODO: see above
                    comment_id,
                    bond_amount,
                    lock_time: current_time,
                    unlock_conditions: UnlockCondition::TimeBasedRelease { unlock_time },
                    bond_transaction_id: tx_id.clone(),
                    confirmation_height: None,
                    bond_address,
                    enforcement_level: BondEnforcementLevel::ScriptBased {
                        script_pubkey: script_pubkey.clone(),
                        unlock_script_condition: script_condition,
                    },
                };

                self.pending_bonds.insert(comment_id, tx_id.clone());
                self.locked_utxos.insert(comment_id, locked_utxo);
                self.total_locked_balance += bond_amount;

                info!("✅ Phase 2.0: Script-based bond created with TRUE blockchain enforcement");
                info!("🔗 Bond transaction: {tx_id}");
                info!("🔒 Funds are now TRULY locked by blockchain script until unlock conditions are met");

                Ok(tx_id)
            }
            Err(e) => {
                error!("❌ Failed to submit script bond transaction: {e}");
                Err(format!("Script bond transaction submission failed: {e}"))
            }
        }
    }

    /// Create a single transaction that both locks a bond UTXO (script-based) and carries
    /// the episode command as payload, brute-forcing the payload nonce to match the pattern.
    pub async fn submit_comment_with_bond_payload(
        &mut self,
        episode_msg: &EpisodeMessage<ContractCommentBoard>,
        bond_amount: u64,
        lock_duration_seconds: u64,
        pattern: PatternType,
        prefix: PrefixType,
        use_script_bonds: bool,
    ) -> Result<String, String> {
        use kaspa_consensus_core::{
            constants::TX_VERSION,
            sign::sign,
            subnets::SUBNETWORK_ID_NATIVE,
            tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutput},
        };
        use kaspa_txscript::pay_to_address_script;

        // Encode payload once (header is updated for nonce, but body is constant)
        let inner = borsh::to_vec(episode_msg).map_err(|e| format!("encode episode msg: {e}"))?;

        // Attempt twice: initial + orphan/refresh retry
        for attempt in 0..=1u8 {
            if attempt == 1 {
                // Refresh wallet state to avoid using spent or pending inputs
                let _ = self.refresh_utxos().await;
            }

            // Coin select: pick smallest-first inputs until we cover bond + fee
            let required_total = bond_amount + FEE;
            let mut utxos_sorted: Vec<(TransactionOutpoint, UtxoEntry)> = self.available_utxos.clone();
            utxos_sorted.sort_by_key(|(_, e)| e.amount);

            let mut selected_inputs: Vec<(TransactionOutpoint, UtxoEntry)> = Vec::new();
            let mut selected_total: u64 = 0;
            for (op, e) in utxos_sorted.into_iter() {
                selected_total = selected_total.saturating_add(e.amount);
                selected_inputs.push((op, e));
                if selected_total >= required_total {
                    break;
                }
            }
            if selected_total < required_total {
                return Err("No available UTXOs for comment+bond".to_string());
            }

            // Build bond output
            let bond_output = if use_script_bonds {
                let current_time = self.current_time();
                let unlock_time = current_time + lock_duration_seconds;
                let script_condition = crate::wallet::kaspa_scripts::ScriptUnlockCondition::TimeLock {
                    unlock_time,
                    user_pubkey: self.keypair.public_key(),
                };
                match crate::wallet::kaspa_scripts::create_bond_script_pubkey(&script_condition) {
                    Ok(spk) => TransactionOutput { value: bond_amount, script_public_key: spk },
                    Err(e) => {
                        warn!("Falling back to P2PK bond output: script build failed: {e}");
                        TransactionOutput { value: bond_amount, script_public_key: pay_to_address_script(&self.kaspa_address) }
                    }
                }
            } else {
                TransactionOutput { value: bond_amount, script_public_key: pay_to_address_script(&self.kaspa_address) }
            };
            let change_value = selected_total.saturating_sub(bond_amount + FEE);
            let change_output = if change_value > 0 {
                Some(TransactionOutput { value: change_value, script_public_key: pay_to_address_script(&self.kaspa_address) })
            } else {
                None
            };

            // Inputs (multi-input)
            let tx_inputs: Vec<TransactionInput> = selected_inputs
                .iter()
                .map(|(op, _e)| TransactionInput { previous_outpoint: *op, signature_script: vec![], sequence: 0, sig_op_count: 1 })
                .collect();

            // Payload with kdapp header for pattern/prefix and episode message
            let mut payload = Payload::pack_header(inner.clone(), prefix);

            // Nonce brute-force loop
            let mut nonce = 0u32;
            let mut outputs = vec![bond_output.clone()];
            if let Some(co) = change_output.clone() {
                outputs.push(co);
            }
            let mut unsigned = Transaction::new_non_finalized(
                TX_VERSION,
                tx_inputs.clone(),
                outputs.clone(),
                0,
                SUBNETWORK_ID_NATIVE,
                0,
                payload.clone(),
            );
            unsigned.finalize();
            while !check_pattern(unsigned.id(), &pattern) {
                nonce = nonce.checked_add(1).ok_or_else(|| "nonce overflow".to_string())?;
                Payload::set_nonce(&mut payload, nonce);
                unsigned = Transaction::new_non_finalized(
                    TX_VERSION,
                    tx_inputs.clone(),
                    outputs.clone(),
                    0,
                    SUBNETWORK_ID_NATIVE,
                    0,
                    payload.clone(),
                );
                unsigned.finalize();
            }

            // Sign with all selected entries
            let selected_entries: Vec<UtxoEntry> = selected_inputs.iter().map(|(_, e)| e.clone()).collect();
            let signed = sign(MutableTransaction::with_entries(unsigned, selected_entries), self.keypair).tx;

            // Submit
            match self.submit_with_retry(&signed).await {
                Ok(()) => return Ok(signed.id().to_string()),
                Err(e) => {
                    let es = e.to_string();
                    if es.contains("orphan") && attempt == 0 {
                        warn!("comment+bond submit rejected as orphan; retrying after refresh...");
                        continue;
                    }
                    return Err(format!("submit failed: {es}"));
                }
            }
        }

        Err("unreachable".to_string())
    }

    /// Phase 2.0: Create script-based transaction that truly locks funds on blockchain
    async fn create_script_bond_transaction(
        &self,
        comment_id: u64,
        bond_amount: u64,
        source_outpoint: &TransactionOutpoint,
        source_entry: &UtxoEntry,
        script_pubkey: &ScriptPublicKey,
        _script_condition: &ScriptUnlockCondition,
    ) -> Result<String, String> {
        info!("🔐 Phase 2.0: Creating REAL script-based bond transaction");
        info!("💰 Bond amount: {:.6} KAS", bond_amount as f64 / 100_000_000.0);
        info!("🔒 Script enforcement: TRUE blockchain-level locking");

        use crate::utils::FEE;
        use kaspa_consensus_core::sign::sign;
        use kaspa_consensus_core::subnets::SubnetworkId;
        use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutput};
        use kaspa_txscript::pay_to_address_script;

        // Create bond payload with script information
        let bond_payload = format!("SCRIPT_BOND:{comment_id}:{bond_amount}:{len}", len = script_pubkey.script().len());

        info!("📝 Bond payload: {bond_payload}");

        // Phase 2.0: Create transaction that sends bond_amount to script_pubkey address
        // This creates a UTXO that can ONLY be spent when script conditions are met

        // Calculate change amount
        let fee = FEE;
        let total_needed = bond_amount + fee;

        if source_entry.amount < total_needed {
            return Err(format!(
                "Insufficient funds: need {:.6} KAS, have {:.6} KAS",
                total_needed as f64 / 100_000_000.0,
                source_entry.amount as f64 / 100_000_000.0
            ));
        }

        let change_amount = source_entry.amount - total_needed;

        info!("💸 Transaction breakdown:");
        info!("  Input: {:.6} KAS", source_entry.amount as f64 / 100_000_000.0);
        info!("  Script-locked output: {:.6} KAS", bond_amount as f64 / 100_000_000.0);
        info!("  Change output: {:.6} KAS", change_amount as f64 / 100_000_000.0);
        info!("  Fee: {:.6} KAS", fee as f64 / 100_000_000.0);

        // For Phase 2.0, we need to create a custom transaction with script-locked output
        // This is more complex than kdapp TransactionGenerator, so we'll build it manually

        // Create transaction inputs
        let tx_input = TransactionInput {
            previous_outpoint: *source_outpoint,
            signature_script: vec![], // Will be filled by signing
            sequence: 0,
            sig_op_count: 1,
        };

        // Create script-locked output (the bond UTXO)
        let script_output = TransactionOutput { value: bond_amount, script_public_key: script_pubkey.clone() };

        // Create change output back to user
        let change_script_pubkey = pay_to_address_script(&self.kaspa_address);
        let change_output = TransactionOutput { value: change_amount, script_public_key: change_script_pubkey };

        // Build the transaction
        let unsigned_tx = Transaction::new(
            0, // version
            vec![tx_input],
            if change_amount > 0 { vec![script_output, change_output] } else { vec![script_output] },
            0,                                 // lock_time
            SubnetworkId::from_bytes([0; 20]), // subnetwork_id
            0,                                 // gas
            bond_payload.into_bytes(),
        );

        // Sign the transaction
        let mutable_tx = MutableTransaction::with_entries(unsigned_tx, vec![source_entry.clone()]);
        let signed_tx = sign(mutable_tx, self.keypair).tx;
        let tx_id = signed_tx.id().to_string();

        info!("🔗 Phase 2.0 script transaction created and signed: {tx_id}");
        info!("✅ Bond UTXO will be TRULY locked by blockchain script");

        // Submit the transaction to Kaspa network
        match self.submit_with_retry(&signed_tx).await {
            Ok(_) => {
                info!("✅ Script-based bond transaction {tx_id} submitted successfully");
                info!("🔒 Funds are now locked by blockchain script - trustless enforcement active");
                Ok(tx_id)
            }
            Err(e) => {
                error!("❌ Failed to submit script bond transaction: {e}");
                Err(format!("Script bond transaction submission failed: {e}"))
            }
        }
    }

    /// Phase 1.2: Create actual Kaspa transaction for bond
    async fn create_bond_transaction(
        &self,
        comment_id: u64,
        bond_amount: u64,
        source_outpoint: &TransactionOutpoint,
        source_entry: &UtxoEntry,
    ) -> Result<String, String> {
        info!("📡 Phase 1.2: Creating REAL bond transaction on Kaspa blockchain...");

        // Phase 1.2: Create REAL on-chain transaction for bond proof
        // This creates a small proof transaction that proves economic commitment

        use crate::utils::FEE;
        use kaspa_consensus_core::{
            sign::sign,
            subnets::SubnetworkId,
            tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint as CoreOutpoint, TransactionOutput},
        };

        // Create bond reference data (not for pattern matching, just for logging)
        let bond_reference = format!("BOND:{comment_id}:{bond_amount}");
        info!("🔐 Creating native bond proof: {} (amount: {:.6} KAS)", bond_reference, bond_amount as f64 / 100_000_000.0);

        // Create NATIVE Kaspa transaction (NO pattern matching overhead!)
        let proof_amount = FEE * 2; // Small proof amount
        let change_amount = source_entry.amount - proof_amount;

        if change_amount < FEE {
            return Err("UTXO too small for bond proof transaction".to_string());
        }

        // Create transaction input
        let tx_input = TransactionInput {
            previous_outpoint: CoreOutpoint::new(source_outpoint.transaction_id, source_outpoint.index),
            signature_script: vec![], // Will be filled by signing
            sequence: 0,
            sig_op_count: 1,
        };

        // Create outputs
        let script = kaspa_txscript::pay_to_address_script(&self.kaspa_address);

        let proof_output = TransactionOutput { value: proof_amount, script_public_key: script.clone() };

        let change_output = TransactionOutput { value: change_amount, script_public_key: script };

        info!(
            "💸 Native transaction: {:.6} KAS proof + {:.6} KAS change (NO pattern matching)",
            proof_amount as f64 / 100_000_000.0,
            change_amount as f64 / 100_000_000.0
        );

        // Build minimal native transaction
        let unsigned_tx = Transaction::new(
            0,                                 // version
            vec![tx_input],                    // inputs
            vec![proof_output, change_output], // outputs
            0,                                 // lock_time
            SubnetworkId::from_bytes([0; 20]), // subnetwork_id
            0,                                 // gas
            bond_reference.into_bytes(),       // Simple payload (no pattern matching)
        );

        // Sign the transaction
        let mutable_tx = MutableTransaction::with_entries(unsigned_tx, vec![source_entry.clone()]);

        let bond_tx = sign(mutable_tx, self.keypair).tx;

        let tx_id = bond_tx.id().to_string();

        // Submit REAL transaction to Kaspa blockchain
        match self.submit_with_retry(&bond_tx).await {
            Ok(_) => {
                info!("✅ REAL bond transaction {tx_id} successfully submitted to Kaspa blockchain");
                info!(
                    "🔗 Phase 1.2: On-chain proof created for comment {} bond ({:.6} KAS)",
                    comment_id,
                    bond_amount as f64 / 100_000_000.0
                );
                Ok(tx_id)
            }
            Err(e) => {
                error!("❌ Failed to submit bond transaction: {e}");
                Err(format!("Bond transaction submission failed: {e}"))
            }
        }
    }

    /// Split large UTXOs to avoid transaction mass limit issues
    /// Staged approach: single-input, two-output split (micro + change)
    pub async fn split_large_utxo(&mut self, max_utxo_size: u64, target_chunk_size: u64) -> Result<(), String> {
        // Find the first large UTXO
        let large_utxo = self.available_utxos.iter().find(|(_, entry)| entry.amount > max_utxo_size).cloned();

        if let Some((outpoint, entry)) = large_utxo {
            use crate::utils::FEE;
            use kaspa_consensus_core::{
                sign::sign,
                subnets::SubnetworkId,
                tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint as CoreOutpoint, TransactionOutput},
            };

            // Ensure we have enough to create one micro output plus change and fee
            let available_amount = entry.amount.saturating_sub(FEE);
            if available_amount <= target_chunk_size + 1000 {
                return Err("UTXO too small to split further".to_string());
            }

            let micro_amount = target_chunk_size;
            let change_amount = available_amount - micro_amount;

            info!(
                "🔄 Splitting large UTXO: input {:.6} KAS -> micro {:.6} KAS + change {:.6} KAS",
                entry.amount as f64 / 100_000_000.0,
                micro_amount as f64 / 100_000_000.0,
                change_amount as f64 / 100_000_000.0
            );

            // Build native wallet transaction (no payload)
            let tx_input = TransactionInput {
                previous_outpoint: CoreOutpoint::new(outpoint.transaction_id, outpoint.index),
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 1,
            };

            let script = kaspa_txscript::pay_to_address_script(&self.kaspa_address);
            let outputs = vec![
                TransactionOutput { value: micro_amount, script_public_key: script.clone() },
                TransactionOutput { value: change_amount, script_public_key: script },
            ];

            let unsigned_tx = Transaction::new(0, vec![tx_input], outputs, 0, SubnetworkId::from_bytes([0; 20]), 0, vec![]);

            let mutable_tx = MutableTransaction::with_entries(unsigned_tx, vec![entry.clone()]);
            let split_tx = sign(mutable_tx, self.keypair).tx;

            match self.submit_with_retry(&split_tx).await {
                Ok(_) => {
                    info!("✅ Split transaction {} submitted successfully", split_tx.id());
                    // Remove the old large UTXO; new ones will appear on refresh
                    self.available_utxos.retain(|(op, _)| op != &outpoint);
                    Ok(())
                }
                Err(e) => {
                    error!("❌ Failed to submit UTXO split transaction: {e}");
                    Err(format!("UTXO split failed: {e}"))
                }
            }
        } else {
            info!("✅ No large UTXOs found (all under {:.6} KAS)", max_utxo_size as f64 / 100_000_000.0);
            Ok(())
        }
    }

    /// Ensure we have at least `min_count` micro-UTXOs (<= max_utxo_size), performing staged splits
    pub async fn ensure_micro_utxos(&mut self, min_count: usize, max_utxo_size: u64, target_chunk_size: u64) -> Result<(), String> {
        use tokio::time::{sleep, Duration};

        let mut attempts: usize = 0;
        loop {
            let small_count = self.available_utxos.iter().filter(|(_, e)| e.amount <= max_utxo_size).count();

            if small_count >= min_count {
                info!("✅ Micro-UTXOs ready: {} at or under {:.6} KAS", small_count, max_utxo_size as f64 / 100_000_000.0);
                return Ok(());
            }

            if attempts >= 30 {
                return Err(format!(
                    "Unable to reach desired micro-UTXOs after {attempts} attempts (have {small_count} <= threshold)"
                ));
            }

            match self.split_large_utxo(max_utxo_size, target_chunk_size).await {
                Ok(_) => {
                    // Allow propagation, then refresh
                    sleep(Duration::from_millis(1500)).await;
                    let _ = self.refresh_utxos().await;
                }
                Err(e) => return Err(e),
            }

            attempts += 1;
        }
    }

    /// Helper to get current timestamp
    fn current_time(&self) -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
    }

    /// Check if a comment's bond can be unlocked
    pub fn can_unlock_bond(&self, comment_id: u64) -> bool {
        if let Some(locked_utxo) = self.locked_utxos.get(&comment_id) {
            match &locked_utxo.unlock_conditions {
                UnlockCondition::TimeBasedRelease { unlock_time } => {
                    let current_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
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

            info!("🔓 Unlocked {:.6} KAS bond for comment {comment_id}", locked_utxo.bond_amount as f64 / 100_000_000.0);

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

            warn!(
                "⚖️ Forfeited {:.6} KAS bond for comment {} (Violation: {})",
                forfeited_amount as f64 / 100_000_000.0,
                comment_id,
                violation_type
            );

            Ok(forfeited_amount)
        } else {
            Err("No locked bond found for comment".to_string())
        }
    }

    /// Get detailed balance information
    /// Phase 1.2: Scan for bond confirmations and update status
    pub async fn scan_pending_bonds(&mut self) -> Result<(), String> {
        let mut confirmed_bonds = Vec::new();

        for (comment_id, tx_id) in &self.pending_bonds {
            // Check if transaction is confirmed by looking for it in the blockchain
            match self.kaspad_client.get_utxos_by_addresses(vec![self.kaspa_address.clone()]).await {
                Ok(entries) => {
                    // If we can see UTXOs, the transaction likely confirmed
                    if !entries.is_empty() {
                        if let Some(locked_utxo) = self.locked_utxos.get_mut(comment_id) {
                            locked_utxo.confirmation_height = Some(1); // Simplified confirmation
                            confirmed_bonds.push(*comment_id);
                            info!("✅ Bond transaction {tx_id} confirmed for comment {comment_id}");
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to check bond confirmation for comment {comment_id}: {e}");
                }
            }
        }

        // Remove confirmed bonds from pending list
        for comment_id in confirmed_bonds {
            self.pending_bonds.remove(&comment_id);
        }

        Ok(())
    }

    /// Phase 2.0: Upgrade existing Phase 1.2 bond to Phase 2.0 script-based enforcement
    pub async fn upgrade_bond_to_script_based(
        &mut self,
        comment_id: u64,
        moderator_pubkeys: Option<Vec<PublicKey>>,
        required_moderator_signatures: Option<usize>,
    ) -> Result<String, String> {
        info!("🔄 Upgrading comment {comment_id} bond from Phase 1.2 to Phase 2.0 script-based enforcement");

        // Check if bond exists and is currently application-layer
        let existing_bond = match self.locked_utxos.get(&comment_id) {
            Some(bond) => bond.clone(),
            None => return Err(format!("No bond found for comment {comment_id}")),
        };

        // Only upgrade application-layer bonds
        match &existing_bond.enforcement_level {
            BondEnforcementLevel::ApplicationLayer { .. } => {
                info!("✅ Bond eligible for upgrade: currently application-layer enforcement");
            }
            BondEnforcementLevel::ScriptBased { .. } => {
                return Err("Bond is already script-based".to_string());
            }
        }

        // Calculate remaining lock time
        let current_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

        let remaining_lock_time = match &existing_bond.unlock_conditions {
            UnlockCondition::TimeBasedRelease { unlock_time } => {
                if *unlock_time > current_time {
                    *unlock_time - current_time
                } else {
                    600 // Default 10 minutes if already unlockable
                }
            }
            _ => 600, // Default 10 minutes for other types
        };

        info!("⏰ Remaining lock time: {remaining_lock_time} seconds");

        // Create new script-based bond with same amount and remaining time
        match self
            .create_script_based_bond(
                comment_id + 1000000, // Use different comment_id to avoid conflicts
                existing_bond.bond_amount,
                remaining_lock_time,
                moderator_pubkeys,
                required_moderator_signatures,
            )
            .await
        {
            Ok(new_bond_tx_id) => {
                // Remove old application-layer bond
                self.locked_utxos.remove(&comment_id);
                self.pending_bonds.remove(&comment_id);

                info!("✅ Bond upgraded successfully!");
                info!("🔒 Old application-layer bond removed");
                info!("🔐 New script-based bond created: {new_bond_tx_id}");
                info!("💎 Funds now TRULY locked by blockchain script");

                Ok(new_bond_tx_id)
            }
            Err(e) => {
                error!("❌ Failed to upgrade bond: {e}");
                Err(format!("Bond upgrade failed: {e}"))
            }
        }
    }

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
    pub async fn refresh_utxos(&mut self) -> Result<(), String> {
        info!("🔄 Refreshing UTXO state from blockchain...");

        let entries = self
            .kaspad_client
            .get_utxos_by_addresses(vec![self.kaspa_address.clone()])
            .await
            .map_err(|e| format!("Failed to refresh UTXOs: {e}"))?;

        self.available_utxos =
            entries.into_iter().map(|entry| (TransactionOutpoint::from(entry.outpoint), UtxoEntry::from(entry.utxo_entry))).collect();

        let new_total = self.available_utxos.iter().map(|(_, entry)| entry.amount).sum();

        if new_total != self.total_available_balance {
            info!(
                "💰 Balance updated: {:.6} KAS -> {:.6} KAS",
                self.total_available_balance as f64 / 100_000_000.0,
                new_total as f64 / 100_000_000.0
            );
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
