use kdapp::episode::{EpisodeEventHandler, EpisodeId, PayloadMetadata};
use kdapp::pki::PubKey;

use crate::episode::{MerchantCommand, ReceiptEpisode};

pub struct MerchantEventHandler;

impl EpisodeEventHandler<ReceiptEpisode> for MerchantEventHandler {
    fn on_initialize(&self, episode_id: EpisodeId, episode: &ReceiptEpisode) {
        log::info!("episode {episode_id} initialized; merchant_keys={:?}", episode.merchant_keys);
    }

    fn on_command(
        &self,
        episode_id: EpisodeId,
        episode: &ReceiptEpisode,
        cmd: &MerchantCommand,
        authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) {
        log::info!(
            "episode {episode_id}: cmd={:?}, auth={:?}, tx_id={}, at={}",
            cmd,
            authorization,
            metadata.tx_id,
            metadata.accepting_time
        );
        if let MerchantCommand::AckReceipt { .. } = cmd {
            if let Ok(bytes) = borsh::to_vec(episode) {
                let hash = crate::tlv::hash_state(&bytes);
                let mut hex = [0u8; 64];
                let _ = faster_hex::hex_encode(&hash, &mut hex);
                if let Ok(hex_str) = std::str::from_utf8(&hex) {
                    log::info!("watchtower checkpoint: state_hash={hex_str}");
                }
            }
        }
    }

    fn on_rollback(&self, episode_id: EpisodeId, _episode: &ReceiptEpisode) {
        log::warn!("episode {episode_id}: rolled back last command");
    }
}
