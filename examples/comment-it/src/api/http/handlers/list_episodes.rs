use crate::api::http::state::PeerState;
use crate::api::http::types::{EpisodeInfo, ListEpisodesResponse};
use axum::{extract::State, http::StatusCode, Json};

pub async fn list_episodes(State(state): State<PeerState>) -> Result<Json<ListEpisodesResponse>, StatusCode> {
    let episodes = state.blockchain_episodes.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let episode_list = episodes
        .iter()
        .map(|(id, episode)| EpisodeInfo {
            episode_id: *id,
            room_code: crate::core::episode::AuthWithCommentsEpisode::generate_room_code(*id),
            creator_public_key: episode.owner().as_ref().map(|pk| pk.to_string()).unwrap_or_default(),
            is_authenticated: episode.is_authenticated(),
        })
        .collect();

    Ok(Json(ListEpisodesResponse { episodes: episode_list }))
}
