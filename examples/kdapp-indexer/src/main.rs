mod api;
mod listener;
mod models;
mod storage;

use axum::http::HeaderValue;
use axum::{routing::get, Router};
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Storage (in-memory by default; swap to RocksDB later)
    let store = storage::new_store()?;

    // Read network/RPC configuration from environment
    let wrpc_url = std::env::var("INDEX_WRPC_URL").ok();
    let network = std::env::var("INDEX_NETWORK").unwrap_or_else(|_| "tn10".to_string());
    let network_id = match network.as_str() {
        "mainnet" => kaspa_wrpc_client::prelude::NetworkId::with_suffix(kaspa_wrpc_client::prelude::NetworkType::Mainnet, 0),
        _ => kaspa_wrpc_client::prelude::NetworkId::with_suffix(kaspa_wrpc_client::prelude::NetworkType::Testnet, 10),
    };

    // Start listener with graceful shutdown support
    let exit_signal = Arc::new(AtomicBool::new(false));
    let store_clone = store.clone();
    let exit_clone = exit_signal.clone();
    tokio::spawn(async move {
        if let Err(e) = listener::run_with_config(store_clone, network_id, wrpc_url, exit_clone).await {
            eprintln!("indexer listener error: {e}");
        }
    });

    // HTTP API
    let cors = build_cors_from_env();
    let app = Router::new()
        .route("/index/health", get(api::health))
        .route("/index/metrics", get(api::metrics))
        .route("/index/recent", get(api::recent_episodes))
        .route("/index/episode/{id}", get(api::episode_snapshot))
        .route("/index/comments/{id}", get(api::episode_comments))
        .route("/index/my-episodes/{pubkey}", get(api::my_episodes))
        .route("/index/me", get(api::me_qs))
        .route("/index/me/{id}", get(api::me))
        .with_state(store)
        .layer(cors);

    let addr: SocketAddr = "0.0.0.0:8090".parse()?;
    println!("kdapp-indexer on http://{addr}/");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let shutdown = async move {
        let _ = tokio::signal::ctrl_c().await;
        exit_signal.store(true, Ordering::Relaxed);
    };
    axum::serve(listener, app).with_graceful_shutdown(shutdown).await?;
    Ok(())
}

fn build_cors_from_env() -> CorsLayer {
    if let Ok(origins) = std::env::var("INDEX_CORS_ORIGINS") {
        let list = origins.split(',').filter_map(|s| HeaderValue::from_str(s.trim()).ok()).collect::<Vec<_>>();
        if !list.is_empty() {
            return CorsLayer::new().allow_origin(list).allow_methods(tower_http::cors::AllowMethods::any()).allow_headers(Any);
        }
    }
    CorsLayer::new().allow_origin(Any).allow_methods(tower_http::cors::AllowMethods::any()).allow_headers(Any)
}
