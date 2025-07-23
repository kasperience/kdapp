use tokio::sync::mpsc::UnboundedSender;
use kdapp::{
    episode::{EpisodeEventHandler, EpisodeId},
    pki::PubKey,
};
use crate::comments::{CommentBoard, CommentState};

pub struct CommentHandler {
    pub sender: UnboundedSender<(EpisodeId, CommentState)>,
    pub participant: PubKey, // The local participant pubkey
}

impl EpisodeEventHandler<CommentBoard> for CommentHandler {
    fn on_initialize(&self, episode_id: kdapp::episode::EpisodeId, episode: &CommentBoard) {
        // Anyone can listen to any room - it's like a public stream!
        let _ = self.sender.send((episode_id, episode.poll()));
    }

    fn on_command(
        &self,
        episode_id: kdapp::episode::EpisodeId,
        episode: &CommentBoard,
        _cmd: &<CommentBoard as kdapp::episode::Episode>::Command,
        _authorization: Option<PubKey>,
        _metadata: &kdapp::episode::PayloadMetadata,
    ) {
        // Send updates for any room activity - like watching a live stream
        let _ = self.sender.send((episode_id, episode.poll()));
    }

    fn on_rollback(&self, _episode_id: kdapp::episode::EpisodeId, _episode: &CommentBoard) {}
}