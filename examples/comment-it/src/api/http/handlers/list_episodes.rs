
use axum::{extract::State, Json};
use crate::api::http::state::PeerState;
use crate::api::http::types::{EpisodeInfo, ListEpisodesResponse};

pub async fn list_episodes(
    State(state): State<PeerState>,
) -> Json<ListEpisodesResponse> {
    let episodes = state.blockchain_episodes.lock().unwrap();
    let episode_list = episodes
        .iter()
        .map(|(id, episode)| EpisodeInfo {
            episode_id: *id,
            creator_public_key: episode.owner().as_ref().map(|pk| pk.to_string()).unwrap_or_default(),
            is_authenticated: episode.is_authenticated(),
        })
        .collect();

    Json(ListEpisodesResponse {
        episodes: episode_list,
    })
}
