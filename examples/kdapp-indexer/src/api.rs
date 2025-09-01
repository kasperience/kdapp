#![allow(dead_code)]
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::Json,
};
use serde::Serialize;

use crate::models::{EpisodeDetail, EpisodeSnapshot};
use crate::storage::Store;

#[derive(Clone)]
pub struct AppState(pub Store);

#[derive(Serialize)]
pub struct RecentResp {
    pub episodes: Vec<EpisodeSnapshot>,
}

#[derive(Serialize)]
pub struct CommentsResp {
    pub comments: Vec<crate::models::CommentRow>,
}

pub async fn health() -> &'static str {
    "ok"
}

#[derive(serde::Deserialize)]
pub struct RecentQuery {
    pub limit: Option<usize>,
}

pub async fn recent_episodes(
    axum::extract::State(store): axum::extract::State<Store>,
    Query(q): Query<RecentQuery>,
) -> Result<Json<RecentResp>, StatusCode> {
    let limit = q.limit.unwrap_or(50).min(500);
    let eps = store.get_recent(limit).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(RecentResp { episodes: eps }))
}

pub async fn episode_snapshot(
    axum::extract::State(store): axum::extract::State<Store>,
    Path(id): Path<u64>,
) -> Result<Json<Option<EpisodeDetail>>, StatusCode> {
    let ep = store.get_episode(id, 50).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ep))
}

#[derive(serde::Deserialize)]
pub struct CommentsQuery {
    pub after_ts: Option<u64>,
    pub limit: Option<usize>,
}

pub async fn episode_comments(
    axum::extract::State(store): axum::extract::State<Store>,
    Path(id): Path<u64>,
    Query(q): Query<CommentsQuery>,
) -> Result<Json<CommentsResp>, StatusCode> {
    let after_ts = q.after_ts.unwrap_or(0);
    let limit = q.limit.unwrap_or(200).min(1000);
    let comments = store.get_comments_after(id, after_ts, limit).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(CommentsResp { comments }))
}

pub async fn my_episodes(
    axum::extract::State(store): axum::extract::State<Store>,
    Path(pubkey): Path<String>,
) -> Result<Json<Vec<u64>>, StatusCode> {
    let eps = store.get_my_episodes(&pubkey, 100).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(eps))
}

// API consistency: GET /index/me/{episode_id}?pubkey=...
#[derive(serde::Deserialize)]
pub struct MeQuery {
    pub pubkey: String,
}

#[derive(Serialize)]
pub struct MeResp {
    pub member: bool,
}

pub async fn me(
    axum::extract::State(store): axum::extract::State<Store>,
    Path(id): Path<u64>,
    Query(q): Query<MeQuery>,
) -> Result<Json<MeResp>, StatusCode> {
    let eps = store.get_my_episodes(&q.pubkey, 1000).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(MeResp { member: eps.contains(&id) }))
}

// Fallback variant: allow /index/me?id=...&pubkey=...
#[derive(serde::Deserialize)]
pub struct MeQueryQS {
    pub id: Option<u64>,
    pub pubkey: Option<String>,
}

pub async fn me_qs(
    axum::extract::State(store): axum::extract::State<Store>,
    Query(q): Query<MeQueryQS>,
) -> Result<Json<MeResp>, StatusCode> {
    let id = q.id.ok_or(StatusCode::BAD_REQUEST)?;
    let pubkey = q.pubkey.as_deref().ok_or(StatusCode::BAD_REQUEST)?;
    let eps = store.get_my_episodes(pubkey, 1000).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(MeResp { member: eps.contains(&id) }))
}

// Simple metrics for observability
#[derive(Serialize)]
pub struct MetricsResp {
    pub episodes: usize,
    pub comments: usize,
    pub memberships: usize,
}

pub async fn metrics(axum::extract::State(store): axum::extract::State<Store>) -> Result<Json<MetricsResp>, StatusCode> {
    let s = store.stats().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(MetricsResp { episodes: s.episodes, comments: s.comments, memberships: s.memberships }))
}
