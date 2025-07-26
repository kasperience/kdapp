use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::{
    network::{NetworkId, NetworkType},
    tx::{TransactionOutpoint, UtxoEntry},
};
use kaspa_wrpc_client::prelude::*;
use log::*;
use rand::Rng;
use secp256k1::{Keypair, SecretKey, Message};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::channel,
        Arc,
    },
};
use tokio::sync::mpsc::UnboundedReceiver;

use kdapp::{
    engine::{self, EpisodeMessage},
    episode::EpisodeId,
    generator::{self, PatternType, PrefixType},
    pki::PubKey,
    proxy::{self, connect_client},
};

use crate::{
    cli::Args,
    comments::CommentCommand,
    episode::{
        handler::CommentHandler,
        board_with_contract::{ContractCommentBoard, ContractState},
        commands::ContractCommand,
    },
    utils::{PATTERN, PREFIX, FEE},
    wallet::UtxoLockManager,
};

pub async fn run_participant(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    // Select network
    let (network, prefix) = if args.mainnet {
        (NetworkId::new(NetworkType::Mainnet), Prefix::Mainnet)
    } else {
        (NetworkId::with_suffix(NetworkType::Testnet, 10), Prefix::Testnet)
    };

    // Generate or obtain Kaspa private key
    let kaspa_signer = if let Some(ref private_key_hex) = args.kaspa_private_key {
        let mut private_key_bytes = [0u8; 32];
        faster_hex::hex_decode(private_key_hex.as_bytes(), &mut private_key_bytes).unwrap();
        Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).unwrap()
    } else {
        let (sk, pk) = &secp256k1::generate_keypair(&mut rand::thread_rng());
        info!(
            "Generated private key {} and address {}. Send some funds to this address and rerun with `--kaspa-private-key {}`",
            sk.display_secret(),
            String::from(&Address::new(prefix, Version::PubKey, &pk.x_only_public_key().0.serialize())),
            sk.display_secret()
        );
        return Ok(());
    };

    // Extract Kaspa address
    let kaspa_addr = Address::new(prefix, Version::PubKey, &kaspa_signer.x_only_public_key().0.serialize());

    // Extract participant identity from Kaspa key (public key is your username!)
    let participant_pk = PubKey(kaspa_signer.public_key());
    let participant_sk = kaspa_signer.secret_key();
    
    info!("Your identity (public key): {}", participant_pk);
    info!("Your Kaspa address: {}", kaspa_addr);

    // Room joining mode
    let target_episode_id = args.room_episode_id;

    // Connect kaspad clients
    let kaspad = connect_client(network, args.wrpc_url.clone()).await.unwrap();
    let participant_kaspad = connect_client(network, args.wrpc_url.clone()).await.unwrap();

    // Define channels and exit flag
    let (sender, receiver) = channel();
    let (response_sender, response_receiver) = tokio::sync::mpsc::unbounded_channel();
    let exit_signal = Arc::new(AtomicBool::new(false));
    let exit_signal_receiver = exit_signal.clone();

    // Run the engine with Episode Contract
    let mut engine = engine::Engine::<ContractCommentBoard, CommentHandler>::new(receiver);
    let engine_task = tokio::task::spawn_blocking(move || {
        engine.start(vec![CommentHandler { sender: response_sender, participant: participant_pk }]);
    });

    // Clone args for the async task
    let args_clone = Args {
        kaspa_private_key: args.kaspa_private_key.clone(),
        room_episode_id: args.room_episode_id,
        mainnet: args.mainnet,
        wrpc_url: args.wrpc_url.clone(),
        log_level: args.log_level.clone(),
        forbidden_words: args.forbidden_words.clone(),
        bonds: args.bonds,
    };

    // Run the participant task
    let participant_task = tokio::spawn(async move {
        run_comment_board(
            participant_kaspad, 
            kaspa_signer, 
            kaspa_addr, 
            response_receiver, 
            exit_signal, 
            participant_sk, 
            participant_pk, 
            target_episode_id,
            args_clone,
        ).await;
    });

    // Run the kaspad listener
    proxy::run_listener(kaspad, std::iter::once((PREFIX, (PATTERN, sender))).collect(), exit_signal_receiver).await;

    engine_task.await.unwrap();
    participant_task.await.unwrap();
    
    Ok(())
}

async fn run_comment_board(
    kaspad: KaspaRpcClient,
    kaspa_signer: Keypair,
    kaspa_addr: Address,
    mut response_receiver: UnboundedReceiver<(EpisodeId, ContractState)>,
    exit_signal: Arc<AtomicBool>,
    participant_sk: SecretKey,
    participant_pk: PubKey,
    target_episode_id: Option<u32>,
    args: Args,
) {
    let entries = kaspad.get_utxos_by_addresses(vec![kaspa_addr.clone()]).await.unwrap();
    if entries.is_empty() {
        println!("❌ No funds found in wallet!");
        println!("💰 Your address: {}", kaspa_addr);
        println!("🚰 Get testnet KAS from: https://faucet.kaspanet.io/");
        println!("🔗 Check balance: https://explorer-tn10.kaspa.org/addresses/{}", kaspa_addr);
        println!("⏳ Wait 1-2 minutes after requesting from faucet, then try again");
        return;
    }
    let entry = entries.first().cloned();
    let mut utxo = entry.map(|entry| (TransactionOutpoint::from(entry.outpoint), UtxoEntry::from(entry.utxo_entry))).unwrap();

    // Initialize UTXO lock manager for real economic enforcement - Phase 1.1
    let mut utxo_manager = UtxoLockManager::new(&kaspad, kaspa_addr.clone(), kaspa_signer).await.unwrap();
    info!("🏦 Wallet initialized with {:.6} KAS available", utxo_manager.get_available_balance() as f64 / 100_000_000.0);

    let generator = generator::TransactionGenerator::new(kaspa_signer, PATTERN, PREFIX);

    let episode_id = if let Some(room_id) = target_episode_id {
        println!("🎯 Joining room with Episode ID: {}", room_id);
        println!("🔧 Registering episode with local engine for command processing...");
        println!("💰 You pay for your own comments with address: {}", kaspa_addr);
        
        // Create episode registration transaction - empty participants means no state initialization
        // This allows the engine to recognize the episode_id when processing commands
        let register_episode = EpisodeMessage::<ContractCommentBoard>::NewEpisode { 
            episode_id: room_id, 
            participants: vec![] // Empty - engine registers episode_id but doesn't call initialize()
        };
        let tx = generator.build_command_transaction(utxo, &kaspa_addr, &register_episode, FEE);
        info!("Submitting episode registration for room {}: {}", room_id, tx.id());
        let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await.unwrap();
        utxo = generator::get_first_output_utxo(&tx);
        room_id
    } else {
        // Create new room - organizer creates the episode
        let new_episode_id = rand::thread_rng().gen();
        println!("🚀 Creating new room with Episode ID: {}", new_episode_id);
        println!("📢 Share this Episode ID with friends to let them join!");
        println!("⚠️  IMPORTANT: Friends must start their terminals BEFORE you create this room!");
        println!("💰 You pay for room creation with address: {}", kaspa_addr);
        
        let new_episode = EpisodeMessage::<ContractCommentBoard>::NewEpisode { 
            episode_id: new_episode_id, 
            participants: vec![] // Empty - anyone can join by sending commands!
        };
        let tx = generator.build_command_transaction(utxo, &kaspa_addr, &new_episode, FEE);
        info!("Submitting room creation: {}", tx.id());
        let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await.unwrap();
        utxo = generator::get_first_output_utxo(&tx);
        new_episode_id
    };

    let (received_episode_id, mut state) = response_receiver.recv().await.unwrap();
    println!("📺 Connected to room: Episode {}", received_episode_id);
    
    // Display simple comment board
    println!("=== 💬 Comment Board ===");
    println!("Comments: {} | Members: {}", state.comments.len(), state.room_members.len());
    for comment in &state.comments {
        println!("[{}] {}: {}", 
            comment.timestamp, 
            &comment.author[..8], 
            comment.text
        );
    }
    println!("========================");

    // TODO: Forbidden words feature temporarily disabled (not working properly)
    // Will be restored in future commit once authorization issues are resolved
    /*
    if let Some(forbidden_words_str) = &args.forbidden_words {
        let forbidden_words: Vec<String> = forbidden_words_str
            .split(',')
            .map(|w| w.trim().to_string())
            .collect();
        
        println!("🚫 Setting forbidden words: {:?}", forbidden_words);
        let forbidden_cmd = CommentCommand::SetForbiddenWords { words: forbidden_words };
        let step = EpisodeMessage::<CommentBoard>::new_signed_command(episode_id, forbidden_cmd, participant_sk, participant_pk);

        let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, FEE);
        info!("💰 Submitting forbidden words (you pay): {}", tx.id());
        let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await.unwrap();
        utxo = generator::get_first_output_utxo(&tx);

        // Wait for confirmation
        loop {
            let (received_id, new_state) = response_receiver.recv().await.unwrap();
            if received_id == episode_id {
                state = new_state;
                println!("✅ Forbidden words set successfully!");
                break;
            }
        }
    }
    */

    // Join the room if not already a member
    if !state.room_members.contains(&format!("{}", participant_pk)) {
        println!("🎉 Joining the room... (paying with your own wallet)");
        let bond_amount = if args.bonds { if state.room_rules.bonds_enabled { state.room_rules.min_bond } else { 0 } } else { 0 };
        if args.bonds {
            println!("💸 Joining room with a {} KAS bond...", bond_amount / 100_000_000);
        } else {
            println!("💬 Joining room (no bond)...");
        }
        let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(episode_id, ContractCommand::JoinRoom { bond_amount }, participant_sk, participant_pk);

        let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, FEE);
        info!("💰 Submitting join room (you pay): {}", tx.id());
        let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await.unwrap();
        utxo = generator::get_first_output_utxo(&tx);

        // Wait for join confirmation
        loop {
            let (received_id, new_state) = response_receiver.recv().await.unwrap();
            if received_id == episode_id {
                state = new_state;
                if state.room_members.contains(&format!("{}", participant_pk)) {
                    println!("✅ Successfully joined the room!");
                    break;
                }
            }
        }
    } else {
        println!("🎯 Already in the room!");
    }

    let mut received_id = received_episode_id;
    let mut input = String::new();

    // --- Authentication Flow ---
    if !state.authenticated_users.contains(&format!("{}", participant_pk)) {
        println!("🔑 Requesting authentication challenge...");
        let request_challenge_cmd = CommentCommand::RequestChallenge;
        let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(episode_id, ContractCommand::RequestChallenge, participant_sk, participant_pk);

        let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, FEE);
        info!("💰 Submitting RequestChallenge (you pay): {}", tx.id());
        let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await.unwrap();
        utxo = generator::get_first_output_utxo(&tx);

        // Wait for challenge
        let mut challenge: Option<String> = None;
        loop {
            (received_id, state) = response_receiver.recv().await.unwrap();
            if received_id == episode_id {
                if let Some(c) = &state.current_challenge {
                    challenge = Some(c.clone());
                    println!("✅ Received challenge: {}", c);
                    break;
                }
            }
        }

        // Sign the challenge and submit response
        if let Some(challenge_text) = challenge {
            println!("✍️ Signing challenge and submitting response...");
            use sha2::{Digest, Sha256};
            let secp = secp256k1::Secp256k1::new();
            let mut hasher = Sha256::new();
            hasher.update(challenge_text.as_bytes());
            let message = Message::from_digest(hasher.finalize().into());
            let signature = secp.sign_ecdsa(&message, &participant_sk);
            let submit_response_cmd = CommentCommand::SubmitResponse {
                signature: signature.to_string(),
                nonce: challenge_text.clone(),
            };
            let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(episode_id, ContractCommand::SubmitResponse { signature: signature.to_string(), nonce: challenge_text }, participant_sk, participant_pk);

            let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, FEE);
            info!("💰 Submitting SubmitResponse (you pay): {}", tx.id());
            let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await.unwrap();
            utxo = generator::get_first_output_utxo(&tx);

            // Wait for authentication confirmation
            loop {
                (received_id, state) = response_receiver.recv().await.unwrap();
                if received_id == episode_id {
                    if state.authenticated_users.contains(&format!("{}", participant_pk)) {
                        println!("✅ Successfully authenticated!");
                        break;
                    }
                }
            }
        } else {
            println!("❌ Failed to get challenge. Cannot authenticate.");
            exit_signal.store(true, Ordering::Relaxed);
            return;
        }
    } else {
        println!("🎯 Already authenticated!");
    }
    // --- End Authentication Flow ---

    loop {
        // Display current state
        if received_id == episode_id {
            // Display simple comment board
            println!("=== 💬 Comment Board ===");
            println!("Comments: {} | Members: {}", state.comments.len(), state.room_members.len());
            for comment in &state.comments {
                println!("[{}] {}: {}", 
                    comment.timestamp, 
                    &comment.author[..8], 
                    comment.text
                );
            }
            println!("========================");
        }

        // Get user input
        input.clear();
        println!("Enter your comment (or 'quit', 'balance', 'unlock', 'bonds', 'upgrade', 'script-bond'):");
        std::io::stdin().read_line(&mut input).unwrap();
        let comment_text = input.trim();

        if comment_text == "quit" {
            exit_signal.store(true, Ordering::Relaxed);
            break;
        }

        // Handle special commands
        if comment_text == "balance" {
            // Refresh and display current balance
            if let Err(e) = utxo_manager.refresh_utxos(&kaspad).await {
                warn!("Failed to refresh UTXOs: {}", e);
            }
            let balance_info = utxo_manager.get_balance_info();
            balance_info.display();
            continue;
        }

        if comment_text == "unlock" {
            // 🔓 TIME-BASED UTXO UNLOCKING: Check and unlock available bonds
            let mut unlocked_total = 0u64;
            let locked_comment_ids: Vec<u64> = utxo_manager.locked_utxos.keys().copied().collect();
            
            for comment_id in locked_comment_ids {
                if utxo_manager.can_unlock_bond(comment_id) {
                    match utxo_manager.unlock_bond(comment_id) {
                        Ok(unlocked_amount) => {
                            unlocked_total += unlocked_amount;
                            println!("🔓 Unlocked {:.6} KAS bond for comment {}", 
                                     unlocked_amount as f64 / 100_000_000.0, comment_id);
                        }
                        Err(e) => {
                            warn!("Failed to unlock bond for comment {}: {}", comment_id, e);
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
            continue;
        }

        // Phase 2.0: Enhanced bond status with enforcement level details
        if comment_text == "bonds" {
            println!("=== 🔒 Bond Status (Phase 1.2 + 2.0) ===");
            if utxo_manager.locked_utxos.is_empty() {
                println!("No active bonds");
            } else {
                for (comment_id, locked_utxo) in &utxo_manager.locked_utxos {
                    match &locked_utxo.enforcement_level {
                        crate::wallet::utxo_manager::BondEnforcementLevel::ApplicationLayer { proof_transaction_id } => {
                            println!("💬 Comment {}: {:.6} KAS (Phase 1.2 - Application Layer)", 
                                   comment_id, locked_utxo.bond_amount as f64 / 100_000_000.0);
                            println!("  🔗 Proof TX: {}", proof_transaction_id);
                            println!("  ⚠️  Enforcement: Application-layer tracking");
                        }
                        crate::wallet::utxo_manager::BondEnforcementLevel::ScriptBased { script_pubkey, unlock_script_condition } => {
                            println!("🔐 Comment {}: {:.6} KAS (Phase 2.0 - Script Enforced)", 
                                   comment_id, locked_utxo.bond_amount as f64 / 100_000_000.0);
                            println!("  🔒 Script size: {} bytes", script_pubkey.script().len());
                            println!("  ✅ Enforcement: TRUE blockchain script-based locking");
                            match unlock_script_condition {
                                crate::wallet::kaspa_scripts::ScriptUnlockCondition::TimeLock { unlock_time, .. } => {
                                    println!("  ⏰ Unlock time: {} (time-lock only)", unlock_time);
                                }
                                crate::wallet::kaspa_scripts::ScriptUnlockCondition::TimeOrModerator { unlock_time, moderator_pubkeys, required_signatures, .. } => {
                                    println!("  ⏰ Unlock time: {} OR moderator consensus", unlock_time);
                                    println!("  👥 Moderators: {} (require {} signatures)", moderator_pubkeys.len(), required_signatures);
                                }
                                _ => {
                                    println!("  🛡️ Complex unlock conditions");
                                }
                            }
                        }
                    }
                    if let Some(confirmation_height) = locked_utxo.confirmation_height {
                        println!("  ✅ Confirmed at height {}", confirmation_height);
                    } else {
                        println!("  ⏳ Pending confirmation");
                    }
                    println!("  🔗 Explorer: https://explorer-tn10.kaspa.org/txs/{}", locked_utxo.bond_transaction_id);
                }
            }
            println!("=====================");
            continue;
        }

        // Phase 2.0: Upgrade existing Phase 1.2 bonds to Phase 2.0 script-based enforcement
        if comment_text == "upgrade" {
            println!("=== 🔄 Upgrade Bonds to Phase 2.0 ===");
            
            // Find application-layer bonds that can be upgraded
            let upgradeable_bonds: Vec<u64> = utxo_manager.locked_utxos.iter()
                .filter_map(|(comment_id, bond)| {
                    match &bond.enforcement_level {
                        crate::wallet::utxo_manager::BondEnforcementLevel::ApplicationLayer { .. } => Some(*comment_id),
                        _ => None,
                    }
                })
                .collect();
            
            if upgradeable_bonds.is_empty() {
                println!("❌ No Phase 1.2 bonds available for upgrade");
                println!("💡 Only application-layer bonds can be upgraded to script-based enforcement");
            } else {
                println!("🔄 Upgradeable bonds found: {}", upgradeable_bonds.len());
                for comment_id in &upgradeable_bonds {
                    if let Some(bond) = utxo_manager.locked_utxos.get(comment_id) {
                        println!("  💬 Comment {}: {:.6} KAS", comment_id, bond.bond_amount as f64 / 100_000_000.0);
                    }
                }
                
                println!("\n🔐 Upgrading first bond to Phase 2.0 script-based enforcement...");
                if let Some(first_comment_id) = upgradeable_bonds.first() {
                    match utxo_manager.upgrade_bond_to_script_based(*first_comment_id, None, None).await {
                        Ok(new_tx_id) => {
                            println!("✅ Bond upgrade successful!");
                            println!("🔗 New script-based transaction: {}", new_tx_id);
                            println!("🔒 Funds are now TRULY locked by blockchain script");
                            println!("💎 No application trust required - pure cryptographic enforcement");
                        }
                        Err(e) => {
                            println!("❌ Upgrade failed: {}", e);
                        }
                    }
                }
            }
            println!("===============================");
            continue;
        }

        // Phase 2.0: Create new script-based bond directly
        if comment_text == "script-bond" {
            println!("=== 🔐 Create Phase 2.0 Script-Based Bond ===");
            
            let balance_info = utxo_manager.get_balance_info();
            balance_info.display();
            
            if balance_info.available_balance < 100_000_000 { // 1 KAS minimum
                println!("❌ Insufficient balance for script-based bond");
                println!("💰 Minimum: 1.0 KAS, Available: {:.6} KAS", balance_info.available_balance as f64 / 100_000_000.0);
            } else {
                println!("\n🔒 Creating Phase 2.0 script-based bond...");
                println!("💰 Bond amount: 100.000000 KAS");
                println!("⏰ Lock duration: 10 minutes");
                println!("🔐 Enforcement: TRUE blockchain script-based locking");
                
                let next_comment_id = state.total_comments + 1000; // Use high comment ID for testing
                match utxo_manager.create_script_based_bond(
                    next_comment_id,
                    100_000_000, // 100 KAS
                    600, // 10 minutes
                    None, // No moderators for now
                    None,
                ).await {
                    Ok(bond_tx_id) => {
                        println!("✅ Phase 2.0 script-based bond created successfully!");
                        println!("🔗 Transaction ID: {}", bond_tx_id);
                        println!("🔒 Funds are TRULY locked by blockchain script");
                        println!("💎 Zero trust required - cryptographic enforcement only");
                        println!("⏰ Will automatically unlock in 10 minutes");
                        println!("🔗 Explorer: https://explorer-tn10.kaspa.org/txs/{}", bond_tx_id);
                    }
                    Err(e) => {
                        println!("❌ Script-based bond creation failed: {}", e);
                    }
                }
            }
            println!("===============================");
            continue;
        }

        if comment_text.is_empty() {
            println!("Comment cannot be empty!");
            continue;
        }

        // Submit comment with bond based on room rules
        let bond_amount = if args.bonds { if state.room_rules.bonds_enabled { state.room_rules.min_bond } else { 0 } } else { 0 };
        
        // 🔒 REAL ECONOMIC ENFORCEMENT: Check if user can afford the bond
        if bond_amount > 0 {
            if !utxo_manager.can_afford_bond(bond_amount) {
                println!("❌ INSUFFICIENT BALANCE FOR BOND!");
                let balance_info = utxo_manager.get_balance_info();
                balance_info.display();
                println!("💸 Required bond: {:.6} KAS", bond_amount as f64 / 100_000_000.0);
                println!("⚠️  Please fund your wallet or comment without --bonds flag");
                continue;
            }
            println!("💸 Submitting comment with a {:.6} KAS bond...", bond_amount as f64 / 100_000_000.0);
        } else {
            println!("💬 Submitting comment (no bond)...");
        }
        let cmd = ContractCommand::SubmitComment { 
            text: comment_text.to_string(),
            bond_amount,
        };
        let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(episode_id, cmd, participant_sk, participant_pk);

        let tx = generator.build_command_transaction(utxo, &kaspa_addr, &step, FEE);
        info!("💰 Submitting comment (you pay): {}", tx.id());
        let _res = kaspad.submit_transaction(tx.as_ref().into(), false).await.unwrap();
        utxo = generator::get_first_output_utxo(&tx);

        // Wait for comment to be processed
        loop {
            (received_id, state) = response_receiver.recv().await.unwrap();
            if received_id == episode_id {
                // Check if our comment was added
                if let Some(latest_comment) = state.comments.last() {
                    if latest_comment.text == comment_text && latest_comment.author == format!("{}", participant_pk) {
                        println!("✅ Comment added to blockchain!");
                        
                        // 🔒 PHASE 1.1: Create REAL bond transaction on Kaspa blockchain
                        if bond_amount > 0 {
                            match utxo_manager.lock_utxo_for_comment(
                                latest_comment.id, 
                                bond_amount, 
                                600 // 10 minutes lock period for testing
                            ).await {
                                Ok(bond_tx_id) => {
                                    println!("🔒 Created REAL bond transaction {} for comment {} ({:.6} KAS)", 
                                             bond_tx_id, 
                                             latest_comment.id, 
                                             bond_amount as f64 / 100_000_000.0);
                                    println!("⏳ Bond transaction submitted to Kaspa blockchain - awaiting confirmation");
                                    println!("⏰ Bond will unlock in 10 minutes after confirmation (if no disputes)");
                                },
                                Err(e) => {
                                    warn!("Failed to create bond transaction: {}", e);
                                }
                            }
                            
                            // Display updated balance
                            let balance_info = utxo_manager.get_balance_info();
                            println!("💰 Updated balance: {:.6} KAS available, {:.6} KAS locked in bonds", 
                                     balance_info.available_balance as f64 / 100_000_000.0,
                                     balance_info.locked_balance as f64 / 100_000_000.0);
                        }
                        break;
                    }
                }
            }
        }
    }
}