#[path = "../src/webhook.rs"]
mod webhook;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use axum::body::Bytes;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio::net::TcpListener;
use webhook::{post_event, WebhookError, WebhookEvent};

#[derive(Clone)]
struct AppState {
    attempts: Arc<AtomicUsize>,
    secret: Vec<u8>,
}

async fn server_500_then_200(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let mut mac = Hmac::<Sha256>::new_from_slice(&state.secret).unwrap();
    mac.update(&body);
    let expected = hex::encode(mac.finalize().into_bytes());
    let sig = headers.get("X-Signature").unwrap().to_str().unwrap();
    assert_eq!(sig, expected);
    let attempt = state.attempts.fetch_add(1, Ordering::SeqCst) + 1;
    if attempt == 1 {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::OK
    }
}

async fn server_400(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let mut mac = Hmac::<Sha256>::new_from_slice(&state.secret).unwrap();
    mac.update(&body);
    let expected = hex::encode(mac.finalize().into_bytes());
    let sig = headers.get("X-Signature").unwrap().to_str().unwrap();
    assert_eq!(sig, expected);
    state.attempts.fetch_add(1, Ordering::SeqCst);
    StatusCode::BAD_REQUEST
}

#[tokio::test]
async fn retries_on_5xx() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let state = AppState { attempts: attempts.clone(), secret: b"topsecret".to_vec() };
    let app = Router::new().route("/", post(server_500_then_200)).with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app.into_make_service()).await.unwrap() });

    let event = WebhookEvent { event: "paid".into(), invoice_id: 1, amount: 100, timestamp: 1 };
    let url = format!("http://{addr}");
    post_event(&url, b"topsecret", &event).await.unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn no_retry_on_4xx() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let state = AppState { attempts: attempts.clone(), secret: b"topsecret".to_vec() };
    let app = Router::new().route("/", post(server_400)).with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app.into_make_service()).await.unwrap() });

    let event = WebhookEvent { event: "paid".into(), invoice_id: 1, amount: 100, timestamp: 1 };
    let url = format!("http://{addr}");
    let err = post_event(&url, b"topsecret", &event).await.unwrap_err();
    match err {
        WebhookError::Http(400) => {}
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}
