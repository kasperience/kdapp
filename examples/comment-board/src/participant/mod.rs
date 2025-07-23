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
            args_clone
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
    assert!(!entries.is_empty(), "No UTXOs found! Fund your address: {}", kaspa_addr);
    let entry = entries.first().cloned();
    let mut utxo = entry.map(|entry| (TransactionOutpoint::from(entry.outpoint), UtxoEntry::from(entry.utxo_entry))).unwrap();

    let generator = generator::TransactionGenerator::new(kaspa_signer, PATTERN, PREFIX);

    let episode_id = if let Some(room_id) = target_episode_id {
        println!("🎯 Joining room with Episode ID: {}", room_id);
        println!("🔧 Creating local episode to enable participation...");
        println!("💰 You pay for your own comments with address: {}", kaspa_addr);
        
        // Create a local episode to enable participation in existing rooms
        // This solves the kdapp limitation where engines only process new transactions
        let join_episode = EpisodeMessage::<ContractCommentBoard>::NewEpisode { 
            episode_id: room_id, 
            participants: vec![] // Empty - open room
        };
        let tx = generator.build_command_transaction(utxo, &kaspa_addr, &join_episode, FEE);
        info!("Submitting local episode creation for room {}: {}", room_id, tx.id());
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
    
    // Display contract status instead of basic state
    println!("=== 💰 Episode Contract Room ===");
    println!("💬 Comments: {}", state.comments.len());
    println!("👥 Members: {}", state.room_members.len());
    println!("🔒 Total Locked: {:.6} KAS", state.total_locked_value as f64 / 100_000_000.0);
    println!("⚖️ Penalty Pool: {:.6} KAS", state.penalty_pool as f64 / 100_000_000.0);
    for comment in &state.comments {
        println!("[{}] {}: {} (Bond: {:.6} KAS)", 
            comment.timestamp, 
            &comment.author[..8], 
            comment.text,
            comment.bond_amount as f64 / 100_000_000.0
        );
    }
    println!("===============================");

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
        let join_cmd = CommentCommand::JoinRoom;
        let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(episode_id, ContractCommand::SubmitComment { text: "joined".to_string(), bond_amount: 0 }, participant_sk, participant_pk);

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
        let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(episode_id, ContractCommand::SubmitComment { text: "auth".to_string(), bond_amount: 0 }, participant_sk, participant_pk);

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
                nonce: challenge_text,
            };
            let step = EpisodeMessage::<ContractCommentBoard>::new_signed_command(episode_id, ContractCommand::SubmitComment { text: "auth_response".to_string(), bond_amount: 0 }, participant_sk, participant_pk);

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
            // Display contract status
            println!("=== 💰 Episode Contract Room ===");
            println!("💬 Comments: {}", state.comments.len());
            println!("👥 Members: {}", state.room_members.len());
            println!("🔒 Total Locked: {:.6} KAS", state.total_locked_value as f64 / 100_000_000.0);
            for comment in &state.comments {
                println!("[{}] {}: {} (Bond: {:.6} KAS)", 
                    comment.timestamp, 
                    &comment.author[..8], 
                    comment.text,
                    comment.bond_amount as f64 / 100_000_000.0
                );
            }
            println!("===============================");
        }

        // Get user input
        input.clear();
        println!("Enter your comment (or 'quit' to exit):");
        std::io::stdin().read_line(&mut input).unwrap();
        let comment_text = input.trim();

        if comment_text == "quit" {
            exit_signal.store(true, Ordering::Relaxed);
            break;
        }

        if comment_text.is_empty() {
            println!("Comment cannot be empty!");
            continue;
        }

        // DEMO: Submit comment with a 100 KAS bond
        let bond_amount = 10_000_000_000; // 100 KAS in Sompis
        println!("💸 Submitting comment with a {} KAS bond...", bond_amount / 100_000_000);
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
                        break;
                    }
                }
            }
        }
    }
}