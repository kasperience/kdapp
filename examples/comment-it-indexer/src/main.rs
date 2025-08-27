mod models;
mod storage;
mod api;
mod listener;

use axum::{routing::get, Router};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Storage (in-memory by default; swap to RocksDB later)
    let store = storage::new_store()?;

    // Start listener (stub: wires in later)
    let store_clone = store.clone();
    tokio::spawn(async move {
        if let Err(e) = listener::run(store_clone).await {
            eprintln!("indexer listener error: {}", e);
        }
    });

    // HTTP API
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(tower_http::cors::AllowMethods::any()).allow_headers(Any);
    let app = Router::new()
        .route("/index/health", get(api::health))
        .route("/index/recent", get(api::recent_episodes))
        .route("/index/episode/{id}", get(api::episode_snapshot))
        .route("/index/comments/{id}", get(api::episode_comments))
        .route("/index/my-episodes/{pubkey}", get(api::my_episodes))
        .with_state(store)
        .layer(cors);

    let addr: SocketAddr = "0.0.0.0:8090".parse()?;
    println!("comment-it indexer on http://{}/", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
