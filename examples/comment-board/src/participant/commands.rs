use crate::wallet::UtxoLockManager;
use log::warn;

pub async fn handle_command(command: &str, utxo_manager: &mut UtxoLockManager) {
    if command == "balance" {
        if let Err(e) = utxo_manager.refresh_utxos().await {
            warn!("Failed to refresh UTXOs: {e}");
        }
        let balance_info = utxo_manager.get_balance_info();
        balance_info.display();
        return;
    }

    if command == "unlock" {
        let mut unlocked_total = 0u64;
        let locked_comment_ids: Vec<u64> = utxo_manager.locked_utxos.keys().copied().collect();

        for comment_id in locked_comment_ids {
            if utxo_manager.can_unlock_bond(comment_id) {
                match utxo_manager.unlock_bond(comment_id) {
                    Ok(unlocked_amount) => {
                        unlocked_total += unlocked_amount;
                        println!("🔓 Unlocked {:.6} KAS bond for comment {}", unlocked_amount as f64 / 100_000_000.0, comment_id);
                    }
                    Err(e) => {
                        warn!("Failed to unlock bond for comment {comment_id}: {e}");
                    }
                }
            }
        }

        if unlocked_total > 0 {
            println!("✅ Total unlocked: {:.6} KAS", unlocked_total as f64 / 100_000_000.0);
            let balance_info = utxo_manager.get_balance_info();
            balance_info.display();
        } else {
            println!("⏰ No bonds ready to unlock yet. Bonds unlock 10 minutes after posting with no disputes.");
        }
        return;
    }

    if command == "bonds" {
        println!("=== 🔒 Bond Status (Phase 1.2 + 2.0) ===");
        if utxo_manager.locked_utxos.is_empty() {
            println!("No active bonds");
        } else {
            for (comment_id, locked_utxo) in &utxo_manager.locked_utxos {
                match &locked_utxo.enforcement_level {
                    crate::wallet::utxo_manager::BondEnforcementLevel::ApplicationLayer { proof_transaction_id } => {
                        println!(
                            "💬 Comment {}: {:.6} KAS (Phase 1.2 - Application Layer)",
                            comment_id,
                            locked_utxo.bond_amount as f64 / 100_000_000.0
                        );
                        println!("  🔗 Proof TX: {proof_transaction_id}");
                        println!("  ⚠️  Enforcement: Application-layer tracking");
                    }
                    crate::wallet::utxo_manager::BondEnforcementLevel::ScriptBased { script_pubkey, unlock_script_condition } => {
                        println!(
                            "🔐 Comment {}: {:.6} KAS (Phase 2.0 - Script Enforced)",
                            comment_id,
                            locked_utxo.bond_amount as f64 / 100_000_000.0
                        );
                        println!("  🔒 Script size: {} bytes", script_pubkey.script().len());
                        println!("  ✅ Enforcement: TRUE blockchain script-based locking");
                        match unlock_script_condition {
                            crate::wallet::kaspa_scripts::ScriptUnlockCondition::TimeLock { unlock_time, .. } => {
                                println!("  ⏰ Unlock time: {unlock_time} (time-lock only)");
                            }
                            crate::wallet::kaspa_scripts::ScriptUnlockCondition::TimeOrModerator {
                                unlock_time,
                                moderator_pubkeys,
                                required_signatures,
                                ..
                            } => {
                                println!("  ⏰ Unlock time: {unlock_time} OR moderator consensus");
                                println!("  👥 Moderators: {} (require {} signatures)", moderator_pubkeys.len(), required_signatures);
                            }
                            _ => {
                                println!("  🛡️ Complex unlock conditions");
                            }
                        }
                    }
                }
                if let Some(confirmation_height) = locked_utxo.confirmation_height {
                    println!("  ✅ Confirmed at height {confirmation_height}");
                } else {
                    println!("  ⏳ Pending confirmation");
                }
                println!("  🔗 Explorer: https://explorer-tn10.kaspa.org/txs/{}", locked_utxo.bond_transaction_id);
            }
        }
        println!("=====================");
        return;
    }

    if command == "upgrade" {
        // ... logic for upgrade ...
        return;
    }

    if command == "script-bond" {
        // ... logic for script-bond ...
    }
}
