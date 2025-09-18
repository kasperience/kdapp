use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::engine::{EngineMsg, EpisodeMessage};
use kdapp::episode::{Episode, EpisodeError, EpisodeEventHandler, EpisodeId, PayloadMetadata, TxOutputInfo};
use kdapp::pki::PubKey;
use kdapp::proxy::TxStatus;
use kaspa_consensus_core::Hash;
use std::sync::mpsc::Sender;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TestCommand {
    Add(u32),
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct TestRollback {
    previous_value: u32,
}

#[derive(Debug, Error)]
pub enum TestError {
    #[error("command would overflow episode state")]
    Overflow,
}

#[derive(Clone, Debug, Default)]
pub struct TestEpisode {
    pub authorized: Vec<PubKey>,
    value: u32,
    executed: Vec<u32>,
}

impl TestEpisode {
    pub fn value(&self) -> u32 {
        self.value
    }
}

impl Episode for TestEpisode {
    type Command = TestCommand;
    type CommandRollback = TestRollback;
    type CommandError = TestError;

    fn initialize(participants: Vec<PubKey>, _metadata: &PayloadMetadata) -> Self {
        Self { authorized: participants, value: 0, executed: Vec::new() }
    }

    fn execute(
        &mut self,
        cmd: &Self::Command,
        authorization: Option<PubKey>,
        _metadata: &PayloadMetadata,
    ) -> Result<Self::CommandRollback, EpisodeError<Self::CommandError>> {
        if let Some(auth) = authorization {
            if !self.authorized.iter().any(|pk| pk == &auth) {
                return Err(EpisodeError::Unauthorized);
            }
        }

        match cmd {
            TestCommand::Add(delta) => {
                let new_value = self
                    .value
                    .checked_add(*delta)
                    .ok_or_else(|| EpisodeError::InvalidCommand(TestError::Overflow))?;
                let rollback = TestRollback { previous_value: self.value };
                self.value = new_value;
                self.executed.push(*delta);
                Ok(rollback)
            }
        }
    }

    fn rollback(&mut self, rollback: Self::CommandRollback) -> bool {
        if self.executed.pop().is_none() && self.value != rollback.previous_value {
            return false;
        }
        self.value = rollback.previous_value;
        true
    }
}

#[derive(Clone, Debug, Default)]
pub struct HandlerState {
    pub initializations: Vec<InitEvent>,
    pub commands: Vec<CommandEvent>,
    pub rollbacks: Vec<RollbackEvent>,
}

#[derive(Clone, Debug)]
pub struct InitEvent {
    pub episode_id: EpisodeId,
    pub value: u32,
    pub metadata: PayloadMetadata,
}

#[derive(Clone, Debug)]
pub struct CommandEvent {
    pub episode_id: EpisodeId,
    pub command: TestCommand,
    pub value: u32,
    pub authorization: Option<PubKey>,
    pub metadata: PayloadMetadata,
}

#[derive(Clone, Debug)]
pub struct RollbackEvent {
    pub episode_id: EpisodeId,
    pub value: u32,
}

#[derive(Clone, Default)]
pub struct RecordingHandler {
    state: Arc<Mutex<HandlerState>>,
}

impl RecordingHandler {
    pub fn new() -> (Self, Arc<Mutex<HandlerState>>) {
        let state = Arc::new(Mutex::new(HandlerState::default()));
        (Self { state: Arc::clone(&state) }, state)
    }
}

impl EpisodeEventHandler<TestEpisode> for RecordingHandler {
    fn on_initialize(&self, episode_id: EpisodeId, episode: &TestEpisode) {
        let mut guard = self.state.lock().expect("handler state poisoned");
        guard.initializations.push(InitEvent { episode_id, value: episode.value(), metadata: empty_metadata() });
    }

    fn on_command(
        &self,
        episode_id: EpisodeId,
        episode: &TestEpisode,
        cmd: &TestCommand,
        authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) {
        let mut guard = self.state.lock().expect("handler state poisoned");
        guard.commands.push(CommandEvent {
            episode_id,
            command: cmd.clone(),
            value: episode.value(),
            authorization,
            metadata: metadata.clone(),
        });
    }

    fn on_rollback(&self, episode_id: EpisodeId, episode: &TestEpisode) {
        let mut guard = self.state.lock().expect("handler state poisoned");
        guard.rollbacks.push(RollbackEvent { episode_id, value: episode.value() });
    }
}

pub fn handler_state_snapshot(state: &Arc<Mutex<HandlerState>>) -> HandlerState {
    state.lock().expect("handler state poisoned").clone()
}

pub fn hash_from_byte(byte: u8) -> Hash {
    let mut data = [0u8; 32];
    data[31] = byte;
    Hash::from_bytes(data)
}

pub fn next_tx_hash() -> Hash {
    static COUNTER: AtomicU8 = AtomicU8::new(0);
    let byte = COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    hash_from_byte(byte)
}

pub fn send_block(
    tx: &Sender<EngineMsg>,
    accepting_hash: Hash,
    accepting_daa: u64,
    accepting_time: u64,
    entries: Vec<(EpisodeMessage<TestEpisode>, Option<Vec<TxOutputInfo>>, Option<TxStatus>)>,
) {
    let associated_txs = entries
        .into_iter()
        .map(|(msg, outputs, status)| {
            let payload = borsh::to_vec(&msg).expect("serialize episode message");
            (next_tx_hash(), payload, outputs, status)
        })
        .collect();
    let event = EngineMsg::BlkAccepted { accepting_hash, accepting_daa, accepting_time, associated_txs };
    tx.send(event).expect("send block event");
}

pub fn empty_metadata() -> PayloadMetadata {
    PayloadMetadata {
        accepting_hash: Hash::default(),
        accepting_daa: 0,
        accepting_time: 0,
        tx_id: Hash::default(),
        tx_outputs: None,
        tx_status: None,
    }
}
