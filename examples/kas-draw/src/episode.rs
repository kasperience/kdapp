#![allow(clippy::enum_variant_names)]
use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::episode::{Episode, EpisodeError, PayloadMetadata};
use kdapp::pki::PubKey;

// M1-scope: single ticket, single draw, single claim, one participant key.

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum LotteryCommand {
    BuyTicket { numbers: [u8; 5], entry_amount: u64 },
    ExecuteDraw { entropy_source: String },
    ClaimPrize { ticket_id: u64, round: u64 },
    CloseEpisode,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum LotteryRollback {
    UndoBuyTicket { ticket_id: u64 },
    UndoExecuteDraw { round_before: u64, prize_pool_before: u64 },
    UndoClaimPrize { ticket_id: u64 },
    UndoClose,
}

#[derive(thiserror::Error, Debug)]
pub enum LotteryError {
    #[error("invalid numbers")]
    InvalidNumbers,
    #[error("incorrect ticket price")]
    IncorrectPrice,
    #[error("no ticket to claim")]
    NoTicket,
    #[error("draw not ready")]
    DrawNotReady,
    #[error("unauthorized")]
    Unauthorized,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Ticket {
    pub ticket_id: u64,
    pub player: PubKey,
    pub numbers: [u8; 5],
    pub round: u64,
    // MVP: we can link to carrier tx via metadata.tx_id at buy time
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct LotteryEpisode {
    // Config
    pub ticket_price: u64,
    pub numbers_range: (u8, u8), // inclusive
    pub numbers_per_ticket: u8,
    pub draw_interval_secs: u64,

    // Minimal round state (M1)
    pub current_round: u64,
    pub round_start_time: u64,
    pub next_draw_time: u64,
    pub prize_pool: u64,

    // Participants / tickets (M1: single key allowed)
    pub authorized: Vec<PubKey>,
    pub tickets: BTreeMap<u64, Ticket>,
    pub next_ticket_id: u64,
    pub last_winner: Option<u64>,
    pub winner_paid: bool,
    pub paused: bool,
}

impl LotteryEpisode {
    fn validate_numbers(&self, numbers: &[u8; 5]) -> bool {
        if self.numbers_per_ticket != 5 {
            return false;
        }
        let (lo, hi) = self.numbers_range;
        // sorted, in-range, no duplicates
        let mut v = numbers.to_vec();
        v.sort_unstable();
        v.windows(2).all(|w| w[0] < w[1]) && v.iter().all(|&n| n >= lo && n <= hi)
    }
}

impl Episode for LotteryEpisode {
    type Command = LotteryCommand;
    type CommandRollback = LotteryRollback;
    type CommandError = LotteryError;

    fn initialize(participants: Vec<PubKey>, metadata: &PayloadMetadata) -> Self {
        // M1 defaults; can be overridden later by admin in M2
        Self {
            ticket_price: 100_000_000, // 100 KAS atoms
            numbers_range: (1, 35),
            numbers_per_ticket: 5,
            draw_interval_secs: 15, // M1 demo: short interval for quick draw
            current_round: 0,
            round_start_time: metadata.accepting_time,
            next_draw_time: metadata.accepting_time + 15, // align with draw_interval_secs for quick demo
            prize_pool: 0,
            authorized: participants,
            tickets: BTreeMap::new(),
            next_ticket_id: 1,
            last_winner: None,
            winner_paid: false,
            paused: false,
        }
    }

    fn execute(
        &mut self,
        cmd: &Self::Command,
        authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) -> Result<Self::CommandRollback, EpisodeError<Self::CommandError>> {
        if self.paused {
            return Err(EpisodeError::InvalidCommand(LotteryError::Unauthorized));
        }
        match cmd {
            LotteryCommand::BuyTicket { numbers, entry_amount } => {
                // M1: single participant key; enforce provided auth key is allowed
                if let Some(pk) = authorization {
                    if !self.authorized.contains(&pk) {
                        return Err(EpisodeError::Unauthorized);
                    }
                } else {
                    return Err(EpisodeError::Unauthorized);
                }
                if !self.validate_numbers(numbers) {
                    return Err(EpisodeError::InvalidCommand(LotteryError::InvalidNumbers));
                }

                // Validate ticket price using carrier tx summary if available
                if *entry_amount != self.ticket_price {
                    return Err(EpisodeError::InvalidCommand(LotteryError::IncorrectPrice));
                }
                // If proxy provided tx outputs, require at least one output is >= ticket price (M1 relaxed).
                if let Some(outs) = &metadata.tx_outputs {
                    let ok = outs.iter().any(|o| o.value >= self.ticket_price);
                    if !ok {
                        return Err(EpisodeError::InvalidCommand(LotteryError::IncorrectPrice));
                    }
                }

                let id = self.next_ticket_id;
                self.next_ticket_id += 1;
                let ticket = Ticket { ticket_id: id, player: authorization.unwrap(), numbers: *numbers, round: self.current_round };
                self.tickets.insert(id, ticket);
                self.prize_pool = self.prize_pool.saturating_add(*entry_amount);
                Ok(LotteryRollback::UndoBuyTicket { ticket_id: id })
            }
            LotteryCommand::ExecuteDraw { entropy_source: _ } => {
                if metadata.accepting_time < self.next_draw_time {
                    return Err(EpisodeError::InvalidCommand(LotteryError::DrawNotReady));
                }
                // M1: pick a simple winner deterministically using accepting_hash low bits if any tickets exist
                if !self.tickets.is_empty() {
                    let mut ids: Vec<u64> = self.tickets.keys().copied().collect();
                    ids.sort_unstable();
                    let idx = (u64::from_le_bytes(metadata.accepting_hash.as_bytes()[..8].try_into().unwrap()) as usize) % ids.len();
                    let winner_id = ids[idx];
                    self.last_winner = Some(winner_id);
                    self.winner_paid = false;
                }
                // Reset window for next round but keep same round in M1 (single draw)
                self.next_draw_time = metadata.accepting_time + self.draw_interval_secs;
                Ok(LotteryRollback::UndoExecuteDraw { round_before: self.current_round, prize_pool_before: self.prize_pool })
            }
            LotteryCommand::ClaimPrize { ticket_id, round: _ } => {
                // M1: single claim – if last_winner matches, zero out pool
                match (self.last_winner, self.tickets.get(ticket_id)) {
                    (Some(wid), Some(_)) if wid == *ticket_id && !self.winner_paid => {
                        self.prize_pool = 0;
                        self.winner_paid = true;
                        Ok(LotteryRollback::UndoClaimPrize { ticket_id: *ticket_id })
                    }
                    _ => Err(EpisodeError::InvalidCommand(LotteryError::NoTicket)),
                }
            }
            LotteryCommand::CloseEpisode => {
                // Off-chain demo: allow close without auth.
                if self.paused {
                    return Err(EpisodeError::InvalidCommand(LotteryError::Unauthorized));
                }
                self.paused = true;
                Ok(LotteryRollback::UndoClose)
            }
        }
    }

    fn rollback(&mut self, rollback: Self::CommandRollback) -> bool {
        match rollback {
            LotteryRollback::UndoBuyTicket { ticket_id } => {
                if let Some(_t) = self.tickets.remove(&ticket_id) {
                    self.prize_pool = self.prize_pool.saturating_sub(self.ticket_price);
                    true
                } else {
                    false
                }
            }
            LotteryRollback::UndoExecuteDraw { round_before, prize_pool_before } => {
                self.current_round = round_before;
                self.prize_pool = prize_pool_before;
                self.last_winner = None;
                self.winner_paid = false;
                true
            }
            LotteryRollback::UndoClaimPrize { ticket_id: _ } => {
                // M1: restore prize pool to one ticket worth
                self.prize_pool = self.ticket_price;
                self.winner_paid = false;
                true
            }
            LotteryRollback::UndoClose => {
                self.paused = false;
                true
            }
        }
    }
}
