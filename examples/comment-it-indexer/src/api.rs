use axum::{extract::Path, http::StatusCode, response::Json};
use serde::Serialize;

use crate::models::{EpisodeDetail, EpisodeSnapshot};
use crate::storage::Store;

#[derive(Clone)]
pub struct AppState(pub Store);

#[derive(Serialize)]
pub struct RecentResp { pub episodes: Vec<EpisodeSnapshot> }

#[derive(Serialize)]
pub struct CommentsResp { pub comments: Vec<crate::models::CommentRow> }

pub async fn health() -> &'static str { "ok" }

pub async fn recent_episodes(axum::extract::State(store): axum::extract::State<Store>) -> Result<Json<RecentResp>, StatusCode> {
    let eps = store.get_recent(50).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(RecentResp { episodes: eps }))
}

pub async fn episode_snapshot(axum::extract::State(store): axum::extract::State<Store>, Path(id): Path<u64>) -> Result<Json<Option<EpisodeDetail>>, StatusCode> {
    let ep = store.get_episode(id, 50).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ep))
}

pub async fn episode_comments(axum::extract::State(store): axum::extract::State<Store>, Path(id): Path<u64>) -> Result<Json<CommentsResp>, StatusCode> {
    let comments = store.get_comments_after(id, 0, 200).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(CommentsResp { comments }))
}

pub async fn my_episodes(axum::extract::State(store): axum::extract::State<Store>, Path(pubkey): Path<String>) -> Result<Json<Vec<u64>>, StatusCode> {
    let eps = store.get_my_episodes(&pubkey, 100).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(eps))
}
