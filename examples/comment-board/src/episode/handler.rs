use crate::episode::board_with_contract::{ContractCommentBoard, ContractState};
use kdapp::{
    episode::{EpisodeEventHandler, EpisodeId},
    pki::PubKey,
};
use tokio::sync::mpsc::UnboundedSender;

pub struct CommentHandler {
    pub sender: UnboundedSender<(EpisodeId, ContractState)>,
    pub _participant: PubKey,
}

impl EpisodeEventHandler<ContractCommentBoard> for CommentHandler {
    fn on_initialize(&self, episode_id: EpisodeId, episode: &ContractCommentBoard) {
        let _ = self.sender.send((episode_id, episode.poll()));
    }

    fn on_command(
        &self,
        episode_id: EpisodeId,
        episode: &ContractCommentBoard,
        _cmd: &<ContractCommentBoard as kdapp::episode::Episode>::Command,
        _authorization: Option<PubKey>,
        _metadata: &kdapp::episode::PayloadMetadata,
    ) {
        let _ = self.sender.send((episode_id, episode.poll()));
    }

    fn on_rollback(&self, _episode_id: EpisodeId, _episode: &ContractCommentBoard) {}
}

// Remove the old CommentBoard handler since we're using ContractCommentBoard only
