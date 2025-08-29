use kdapp::episode::{EpisodeEventHandler, EpisodeId, PayloadMetadata};
use kdapp::pki::PubKey;

use crate::episode::{LotteryCommand, LotteryEpisode};

pub struct Handler;

impl EpisodeEventHandler<LotteryEpisode> for Handler {
    fn on_initialize(&self, episode_id: EpisodeId, _episode: &LotteryEpisode) {
        log::info!("kas-draw initialized episode {}", episode_id);
    }

    fn on_command(
        &self,
        episode_id: EpisodeId,
        episode: &LotteryEpisode,
        cmd: &LotteryCommand,
        _authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) {
        match cmd {
            LotteryCommand::ExecuteDraw { .. } => {
                log::info!(
                    "kas-draw ep {} DRAW at {} → last_winner={:?}, pool={}",
                    episode_id, metadata.tx_id, episode.last_winner, episode.prize_pool
                );
            }
            LotteryCommand::ClaimPrize { ticket_id, .. } => {
                log::info!(
                    "kas-draw ep {} CLAIM ticket={} at {} → pool={}",
                    episode_id, ticket_id, metadata.tx_id, episode.prize_pool
                );
            }
            LotteryCommand::BuyTicket { numbers, entry_amount } => {
                log::info!(
                    "kas-draw ep {} BUY {:?} amount={} at {} → pool={}",
                    episode_id, numbers, entry_amount, metadata.tx_id, episode.prize_pool
                );
            }
        }
    }

    fn on_rollback(&self, episode_id: EpisodeId, _episode: &LotteryEpisode) {
        log::warn!("kas-draw episode {} rolled back one step", episode_id);
    }
}
