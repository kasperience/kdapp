use crate::core::{AuthWithCommentsEpisode, UnifiedCommand};
use kdapp::episode::{Episode, PayloadMetadata};
use kdapp::pki::{generate_keypair, sign_message, to_message};
use std::error::Error;

pub fn test_episode_logic(participant_count: usize) -> Result<(), Box<dyn Error>> {
    println!("🎯 Testing AuthWithCommentsEpisode Episode Logic");
    println!("Participants: {participant_count}");

    // Generate keypairs for participants
    let mut keypairs = Vec::new();
    let mut pubkeys = Vec::new();

    for i in 0..participant_count {
        let (secret_key, pub_key) = generate_keypair();
        println!("Generated keypair {} for participant: {}", i + 1, pub_key);
        keypairs.push((secret_key, pub_key));
        pubkeys.push(pub_key);
    }

    // Create metadata
    let metadata = PayloadMetadata {
        accepting_hash: 0u64.into(),
        accepting_daa: 0,
        accepting_time: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        tx_id: 1u64.into(),
        tx_outputs: None,
    };

    // Initialize episode
    let mut auth_episode = AuthWithCommentsEpisode::initialize(pubkeys.clone(), &metadata);
    println!("✅ Episode initialized");

    // Test authentication flow for first participant
    let (secret_key, pub_key) = &keypairs[0];

    println!(
        "
🔑 Testing authentication flow for participant: {pub_key}"
    );

    // Step 1: Request challenge
    println!("📨 Requesting challenge...");
    let rollback1 = auth_episode.execute(&UnifiedCommand::RequestChallenge, Some(*pub_key), &metadata)?;

    let challenge = auth_episode.challenge().unwrap();
    println!("🎲 Received challenge: {challenge}");

    // Step 2: Sign challenge
    println!("✍️ Signing challenge...");
    let msg = to_message(&challenge.to_string());
    let signature = sign_message(secret_key, &msg);
    println!("📝 Signature created");

    // Step 3: Submit response
    println!("📤 Submitting signed response...");
    let rollback2 = auth_episode.execute(
        &UnifiedCommand::SubmitResponse { signature: hex::encode(signature.0.serialize_der()), nonce: challenge },
        Some(*pub_key),
        &metadata,
    )?;

    // Check results
    if auth_episode.is_authenticated() {
        println!("✅ Authentication successful!");
        if let Some(ref token) = auth_episode.session_token() {
            println!("🎟️ Session token: {token}");
        }
    } else {
        println!("❌ Authentication failed");
    }

    // Test rollback functionality
    println!(
        "
🔄 Testing rollback functionality..."
    );
    let rollback_success = auth_episode.rollback(rollback2);
    println!("Rollback authentication: {}", if rollback_success { "✅" } else { "❌" });

    let rollback_success = auth_episode.rollback(rollback1);
    println!("Rollback challenge: {}", if rollback_success { "✅" } else { "❌" });

    println!(
        "
🎉 Episode logic test completed successfully!"
    );
    Ok(())
}

pub fn run_interactive_demo() -> Result<(), Box<dyn Error>> {
    println!("🚀 Kaspa Auth Interactive Demo");
    println!("This will simulate a two-party authentication flow");

    // Generate two keypairs (Alice and Bob)
    let (alice_sk, alice_pk) = generate_keypair();
    let (_, bob_pk) = generate_keypair();

    println!(
        "
👥 Participants:"
    );
    println!("Alice (requester): {alice_pk}");
    println!("Bob (verifier): {bob_pk}");

    let metadata = PayloadMetadata {
        accepting_hash: 0u64.into(),
        accepting_daa: 0,
        accepting_time: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        tx_id: 1u64.into(),
        tx_outputs: None,
    };

    // Initialize episode with both participants
    let mut auth_episode = AuthWithCommentsEpisode::initialize(vec![alice_pk, bob_pk], &metadata);

    println!(
        "
📡 Episode initialized on simulated Kaspa network"
    );

    // Alice requests authentication
    println!(
        "
🔐 Alice initiates authentication..."
    );
    let _rollback = auth_episode.execute(&UnifiedCommand::RequestChallenge, Some(alice_pk), &metadata)?;

    let challenge = auth_episode.challenge().unwrap();
    println!("📨 Bob sends challenge to Alice: {challenge}");

    // Alice signs the challenge
    println!("✍️ Alice signs the challenge...");
    let msg = to_message(&challenge.to_string());
    let signature = sign_message(&alice_sk, &msg);

    // Alice submits signed response
    println!("📤 Alice submits signed response to Bob...");
    let _rollback = auth_episode.execute(
        &UnifiedCommand::SubmitResponse { signature: hex::encode(signature.0.serialize_der()), nonce: challenge },
        Some(alice_pk),
        &metadata,
    )?;

    // Show final result
    println!(
        "
🎯 Final Result:"
    );
    if auth_episode.is_authenticated() {
        println!("✅ Alice successfully authenticated!");
        if let Some(ref token) = auth_episode.session_token() {
            println!("🎟️ Session token issued: {token}");
        }
        println!("🎉 Authentication complete - Alice can now access protected resources");
    } else {
        println!("❌ Authentication failed");
    }

    Ok(())
}
