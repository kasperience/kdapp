#![allow(dead_code)]
use crate::wallet::AgentWallet;
use kaspa_wrpc_client::KaspaRpcClient;
use kdapp::engine::{Engine, EngineMsg};
use kdapp::episode::{Episode, EpisodeError, EpisodeEventHandler, PayloadMetadata};
use kdapp::pki::PubKey;
use log::debug;
use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{
    mpsc::{Receiver, Sender},
    Arc, Mutex,
};

// =========================
// TicTacToe Episode (real)
// =========================

#[derive(Clone, Debug, PartialEq)]
pub enum Player {
    X,
    O,
}

#[derive(Clone, Debug)]
pub struct TicTacToeEpisode {
    board: [[u8; 3]; 3], // 0 empty, 1 X, 2 O
    current: Player,
    participants: Vec<PubKey>,
    // Keep a sliding window of the last 6 placed symbols (row, col)
    move_history: VecDeque<(u8, u8)>,
}

#[derive(thiserror::Error, Debug)]
pub enum TicTacToeError {
    #[error("out of bounds")]
    OutOfBounds,
    #[error("cell occupied")]
    CellOccupied,
    #[error("wrong turn")]
    WrongTurn,
}

#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum TttCommand {
    Move { row: u8, col: u8, player: u8 }, // 0->X, 1->O
}

#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct TttRemoved {
    row: u8,
    col: u8,
    symbol: u8,
}

#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct TttRollback {
    row: u8,
    col: u8,
    prev_player: u8,
    removed: Option<TttRemoved>,
}

impl Episode for TicTacToeEpisode {
    type Command = TttCommand;
    type CommandRollback = TttRollback;
    type CommandError = TicTacToeError;

    fn initialize(participants: Vec<PubKey>, _metadata: &PayloadMetadata) -> Self {
        Self { board: [[0; 3]; 3], current: Player::X, participants, move_history: VecDeque::new() }
    }

    fn execute(
        &mut self,
        cmd: &Self::Command,
        authorization: Option<PubKey>,
        _metadata: &PayloadMetadata,
    ) -> Result<Self::CommandRollback, kdapp::episode::EpisodeError<Self::CommandError>> {
        match cmd {
            TttCommand::Move { row, col, player } => {
                let r = *row as usize;
                let c = *col as usize;
                if r > 2 || c > 2 {
                    return Err(kdapp::episode::EpisodeError::InvalidCommand(TicTacToeError::OutOfBounds));
                }
                if self.board[r][c] != 0 {
                    return Err(kdapp::episode::EpisodeError::InvalidCommand(TicTacToeError::CellOccupied));
                }
                let want = match self.current {
                    Player::X => 0u8,
                    Player::O => 1u8,
                };
                if *player != want {
                    return Err(kdapp::episode::EpisodeError::InvalidCommand(TicTacToeError::WrongTurn));
                }
                // Authorization required and must match designated participant for this player
                let auth_pk = authorization.ok_or(EpisodeError::Unauthorized)?;
                let designated = if *player == 0 { self.participants.first() } else { self.participants.get(1) };
                if let Some(expected) = designated {
                    if &auth_pk != expected {
                        return Err(EpisodeError::Unauthorized);
                    }
                } else {
                    return Err(EpisodeError::Unauthorized);
                }
                // Enforce maximum 6 symbols on board via sliding window
                let mut removed: Option<TttRemoved> = None;
                if self.move_history.len() == 6 {
                    if let Some((old_r, old_c)) = self.move_history.pop_front() {
                        let or = old_r as usize;
                        let oc = old_c as usize;
                        let sym = self.board[or][oc];
                        if sym != 0 {
                            self.board[or][oc] = 0;
                            removed = Some(TttRemoved { row: old_r, col: old_c, symbol: sym });
                        }
                    }
                }

                // apply current move
                self.board[r][c] = if *player == 0 { 1 } else { 2 };
                self.move_history.push_back((*row, *col));

                let prev = want;
                self.current = if *player == 0 { Player::O } else { Player::X };
                Ok(TttRollback { row: *row, col: *col, prev_player: prev, removed })
            }
        }
    }

    fn rollback(&mut self, rollback: Self::CommandRollback) -> bool {
        let r = rollback.row as usize;
        let c = rollback.col as usize;
        if r > 2 || c > 2 {
            return false;
        }
        // Remove the last-applied move
        self.board[r][c] = 0;
        if !self.move_history.is_empty() {
            self.move_history.pop_back();
        }

        // Restore any symbol that was evicted due to the 6-move rule
        if let Some(rem) = rollback.removed {
            let rr = rem.row as usize;
            let rc = rem.col as usize;
            if rr <= 2 && rc <= 2 {
                self.board[rr][rc] = rem.symbol;
                // Put it back at the front to preserve original ordering
                self.move_history.push_front((rem.row, rem.col));
            }
        }

        // Restore whose turn it was before the rolled-back move
        self.current = if rollback.prev_player == 0 { Player::X } else { Player::O };
        true
    }
}

// Snapshot for external state reporting
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TttSnapshot {
    pub board: [[u8; 3]; 3], // 0 empty, 1 X, 2 O
    pub current: char,       // 'X' or 'O'
    pub participants: usize,
}

// Event handler that mirrors episode state into shared map
pub struct TttEventHandler {
    pub out: Arc<Mutex<HashMap<kdapp::episode::EpisodeId, TttSnapshot>>>,
    pub dir: PathBuf,
}

impl EpisodeEventHandler<TicTacToeEpisode> for TttEventHandler {
    fn on_initialize(&self, episode_id: kdapp::episode::EpisodeId, episode: &TicTacToeEpisode) {
        let snap = TttSnapshot { board: episode.board, current: 'X', participants: episode.participants.len() };
        if let Ok(mut m) = self.out.lock() {
            m.insert(episode_id, snap.clone());
        }
        self.persist_snapshot(episode_id, &snap, "initialize");
    }
    fn on_command(
        &self,
        episode_id: kdapp::episode::EpisodeId,
        episode: &TicTacToeEpisode,
        _cmd: &TttCommand,
        _authorization: Option<PubKey>,
        _metadata: &PayloadMetadata,
    ) {
        let current = match episode.current {
            Player::X => 'X',
            Player::O => 'O',
        };
        let snap = TttSnapshot { board: episode.board, current, participants: episode.participants.len() };
        if let Ok(mut m) = self.out.lock() {
            m.insert(episode_id, snap.clone());
        }
        self.persist_snapshot(episode_id, &snap, "command");
        debug!("TicTacToe episode {episode_id} updated");
    }
    fn on_rollback(&self, episode_id: kdapp::episode::EpisodeId, episode: &TicTacToeEpisode) {
        let current = match episode.current {
            Player::X => 'X',
            Player::O => 'O',
        };
        let snap = TttSnapshot { board: episode.board, current, participants: episode.participants.len() };
        if let Ok(mut m) = self.out.lock() {
            m.insert(episode_id, snap.clone());
        }
        self.persist_snapshot(episode_id, &snap, "rollback");
        debug!("TicTacToe episode {episode_id} rolled back");
    }
}

impl TttEventHandler {
    fn persist_snapshot(&self, episode_id: kdapp::episode::EpisodeId, snap: &TttSnapshot, event: &str) {
        // Ensure directory exists
        if let Err(e) = fs::create_dir_all(&self.dir) {
            eprintln!("Failed creating episodes dir: {e}");
            return;
        }
        // Write snapshot file
        let path = self.dir.join(format!("{episode_id}.json"));
        if let Ok(json) = serde_json::to_string_pretty(snap) {
            if let Err(e) = fs::write(&path, json) {
                eprintln!("Failed persisting snapshot {}: {}", path.display(), e);
            }
        }
        // Append event to jsonl log
        let log_path = self.dir.join("events.jsonl");
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = writeln!(
                f,
                "{}",
                serde_json::json!({
                    "episode_id": episode_id,
                    "event": event,
                    "ts": chrono::Utc::now().to_rfc3339(),
                    "snapshot": snap,
                })
            );
        }
    }
}

pub struct ServerState {
    pub sender: Sender<EngineMsg>,
    pub ttt_state: Arc<Mutex<HashMap<kdapp::episode::EpisodeId, TttSnapshot>>>,
    pub transaction_generator: Option<kdapp::generator::TransactionGenerator>,
    pub agent1_wallet: Arc<AgentWallet>,
    pub agent2_wallet: Arc<AgentWallet>,
    pub node_client: Option<Arc<KaspaRpcClient>>,
}

impl ServerState {
    pub fn new(agent1_wallet: AgentWallet, agent2_wallet: AgentWallet, node_client: Option<KaspaRpcClient>) -> Self {
        // Create a channel for EngineMsg
        let (sender, receiver): (Sender<EngineMsg>, Receiver<EngineMsg>) = std::sync::mpsc::channel();

        // Shared state map for snapshots
        let ttt_state: Arc<Mutex<HashMap<kdapp::episode::EpisodeId, TttSnapshot>>> = Arc::new(Mutex::new(HashMap::new()));
        let episodes_dir = PathBuf::from("episodes");
        // Preload any existing snapshots from disk for quick reporting
        if let Ok(entries) = fs::read_dir(&episodes_dir) {
            for ent in entries.flatten() {
                if let Some(ext) = ent.path().extension() {
                    if ext == "json" {
                        if let Some(stem) = ent.path().file_stem().and_then(|s| s.to_str()) {
                            if let Ok(eid) = stem.parse::<kdapp::episode::EpisodeId>() {
                                if let Ok(txt) = fs::read_to_string(ent.path()) {
                                    if let Ok(snap) = serde_json::from_str::<TttSnapshot>(&txt) {
                                        if let Ok(mut m) = ttt_state.lock() {
                                            m.insert(eid, snap);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let handler = TttEventHandler { out: ttt_state.clone(), dir: episodes_dir.clone() };

        // Initialize the kdapp engine with the receiver and start in a dedicated thread
        std::thread::spawn(move || {
            let mut engine: Engine<TicTacToeEpisode, TttEventHandler> = Engine::new(receiver);
            engine.start(vec![handler]);
        });

        use crate::routing;
        let keypair = agent1_wallet.keypair;
        let transaction_generator =
            kdapp::generator::TransactionGenerator::new(keypair, routing::PATTERN, routing::PREFIX);

        Self {
            sender,
            ttt_state,
            transaction_generator: Some(transaction_generator),
            agent1_wallet: Arc::new(agent1_wallet),
            agent2_wallet: Arc::new(agent2_wallet),
            node_client: node_client.map(Arc::new),
        }
    }
}
