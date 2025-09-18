use integration_tests::support::{hash_from_byte, TestCommand, TestEpisode};
use kdapp::engine::EpisodeMessage;
use kdapp::generator::{check_pattern, get_first_output_utxo, Payload, TransactionGenerator, PatternType, PrefixType};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_txscript::pay_to_address_script;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use secp256k1::{Keypair, Secp256k1, SecretKey};

fn deterministic_secret(seed: u64) -> SecretKey {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut bytes = [0u8; 32];
    loop {
        rng.fill_bytes(&mut bytes);
        if let Ok(secret) = SecretKey::from_slice(&bytes) {
            return secret;
        }
    }
}

#[test]
fn generator_builds_payload_and_matches_pattern() {
    let secp = Secp256k1::new();
    let secret_key = deterministic_secret(42);
    let keypair = Keypair::from_secret_key(&secp, &secret_key);
    let pattern: PatternType = [(0, 0); 10];
    let prefix: PrefixType = 0xA1B2C3D4;

    let recipient_secret = deterministic_secret(7);
    let recipient_keypair = Keypair::from_secret_key(&secp, &recipient_secret);
    let recipient_address = Address::new(Prefix::Testnet, Version::PubKey, &recipient_keypair.x_only_public_key().0.serialize());

    let owner_address = Address::new(Prefix::Testnet, Version::PubKey, &keypair.x_only_public_key().0.serialize());
    let utxo_amount = 25_000;
    let utxo = (TransactionOutpoint::new(hash_from_byte(50), 0), UtxoEntry::new(utxo_amount, pay_to_address_script(&owner_address), 0, false));

    let generator = TransactionGenerator::new(keypair, pattern, prefix);
    let command = EpisodeMessage::<TestEpisode>::UnsignedCommand { episode_id: 9, cmd: TestCommand::Add(2) };
    let fee = 500;
    let tx = generator.build_command_transaction(utxo.clone(), &recipient_address, &command, fee);

    assert!(check_pattern(tx.id(), &pattern), "transaction id should satisfy test pattern");
    assert!(Payload::check_header(&tx.payload, prefix));

    let payload = Payload::strip_header(tx.payload.clone());
    let decoded: EpisodeMessage<TestEpisode> = borsh::from_slice(&payload).expect("decode payload");
    match decoded {
        EpisodeMessage::UnsignedCommand { episode_id, cmd } => {
            assert_eq!(episode_id, 9);
            assert_eq!(cmd, TestCommand::Add(2));
        }
        other => panic!("unexpected payload variant: {other:?}"),
    }

    assert_eq!(tx.outputs.len(), 1);
    assert_eq!(tx.outputs[0].value, utxo_amount - fee);
    let recipient_script = pay_to_address_script(&recipient_address);
    assert_eq!(tx.outputs[0].script_public_key, recipient_script);

    let expected_first = (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(tx.outputs[0].value, tx.outputs[0].script_public_key.clone(), 0, false));
    let actual_first = get_first_output_utxo(&tx);
    assert_eq!(actual_first.0, expected_first.0);
    assert_eq!(actual_first.1.amount, expected_first.1.amount);
    assert_eq!(actual_first.1.script_public_key, expected_first.1.script_public_key);

    assert_eq!(tx.inputs.len(), 1);
    assert_eq!(tx.inputs[0].previous_outpoint, utxo.0);
}
