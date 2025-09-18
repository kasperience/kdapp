use std::sync::mpsc::channel;
use std::thread;

use integration_tests::support::{
    handler_state_snapshot,
    hash_from_byte,
    send_block,
    HandlerState,
    RecordingHandler,
    TestCommand,
    TestEpisode,
};
use kdapp::engine::{Engine, EngineMsg, EpisodeMessage};
use kdapp::episode::{EpisodeId, TxOutputInfo};
use kdapp::pki::generate_keypair;
use kdapp::proxy::TxStatus;

fn append_block(
    sender: &std::sync::mpsc::Sender<EngineMsg>,
    accepting_hash: u8,
    entries: Vec<(EpisodeMessage<TestEpisode>, Option<Vec<TxOutputInfo>>, Option<TxStatus>)>,
) {
    send_block(
        sender,
        hash_from_byte(accepting_hash),
        accepting_hash as u64 * 100,
        accepting_hash as u64 * 10,
        entries,
    );
}

#[test]
fn engine_processes_signed_unsigned_and_reorgs_without_network() {
    let (authorized_sk, authorized_pk) = generate_keypair();
    let (unauthorized_sk, unauthorized_pk) = generate_keypair();
    let (tx, rx) = channel();
    let (handler, state) = RecordingHandler::new();

    let mut engine = Engine::<TestEpisode, RecordingHandler>::new(rx);
    let engine_handle = thread::spawn(move || {
        engine.start(vec![handler]);
    });

    let episode_id: EpisodeId = 7;

    append_block(
        &tx,
        1,
        vec![
            (
                EpisodeMessage::NewEpisode { episode_id, participants: vec![authorized_pk] },
                Some(vec![TxOutputInfo { value: 100, script_version: 0, script_bytes: Some(vec![1, 2, 3]) }]),
                Some(TxStatus { acceptance_height: Some(10), confirmations: Some(1), finality: Some(false) }),
            ),
        ],
    );

    let signed_cmd = EpisodeMessage::new_signed_command(episode_id, TestCommand::Add(5), authorized_sk, authorized_pk);
    append_block(&tx, 2, vec![(signed_cmd, None, None)]);

    let unauthorized_cmd = EpisodeMessage::new_signed_command(episode_id, TestCommand::Add(3), unauthorized_sk, unauthorized_pk);
    append_block(&tx, 3, vec![(unauthorized_cmd, None, None)]);

    let unsigned_cmd = EpisodeMessage::UnsignedCommand { episode_id, cmd: TestCommand::Add(7) };
    append_block(
        &tx,
        4,
        vec![(
            unsigned_cmd,
            None,
            Some(TxStatus { acceptance_height: Some(20), confirmations: Some(5), finality: Some(true) }),
        )],
    );

    tx.send(EngineMsg::BlkReverted { accepting_hash: hash_from_byte(4) }).expect("send revert");
    tx.send(EngineMsg::BlkReverted { accepting_hash: hash_from_byte(3) }).expect("send revert");
    tx.send(EngineMsg::BlkReverted { accepting_hash: hash_from_byte(2) }).expect("send revert");
    tx.send(EngineMsg::BlkReverted { accepting_hash: hash_from_byte(1) }).expect("send revert");
    tx.send(EngineMsg::Exit).expect("send exit");

    engine_handle.join().expect("engine thread");

    let HandlerState { initializations, commands, rollbacks } = handler_state_snapshot(&state);

    assert_eq!(initializations.len(), 1, "episode should be initialized once");
    assert_eq!(initializations[0].value, 0);

    assert_eq!(commands.len(), 2, "only signed and unsigned commands are accepted");
    assert_eq!(commands[0].command, TestCommand::Add(5));
    assert_eq!(commands[0].value, 5);
    assert_eq!(commands[0].authorization, Some(authorized_pk));
    assert!(commands[0].metadata.tx_status.is_none());

    assert_eq!(commands[1].command, TestCommand::Add(7));
    assert_eq!(commands[1].value, 12);
    assert_eq!(commands[1].authorization, None);
    assert_eq!(commands[1].metadata.tx_status.as_ref().and_then(|s| s.finality), Some(true));

    assert_eq!(rollbacks.len(), 3, "unsigned, signed, and creation rollbacks recorded");
    assert_eq!(rollbacks[0].value, 5, "unsigned rollback returns to signed state");
    assert_eq!(rollbacks[1].value, 0, "signed rollback returns to initial value");
    assert_eq!(rollbacks[2].value, 0, "episode deletion leaves default state");
}
