use std::sync::{mpsc, OnceLock};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use kdapp::pki::generate_keypair;
use kdapp_merchant::server::{self, AppState};
use kdapp_merchant::sim_router::{EngineChannel, SimRouter};
use kdapp_merchant::watcher::{self, MempoolSnapshot, PolicySnapshot, MIN_FEE};
use serde::Deserialize;
use serde_json::Value;
use tower::ServiceExt;
use tokio::sync::Mutex;

async fn test_guard() -> tokio::sync::MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(())).lock().await
}

fn init_policy(max_fee: u64, threshold: f64) {
    watcher::set_mempool_snapshot(MempoolSnapshot {
        est_base_fee: MIN_FEE,
        congestion_ratio: 0.0,
        min_fee: MIN_FEE,
        max_fee,
    });
    let policy_name = if (threshold - 1.0).abs() < f64::EPSILON {
        "static"
    } else {
        "congestion"
    };
    watcher::set_policy_snapshot(PolicySnapshot {
        min: MIN_FEE,
        max: max_fee,
        policy: policy_name.to_string(),
        selected_fee: max_fee,
        deferred: false,
        congestion_threshold: threshold,
    });
}

fn test_state() -> AppState {
    let (sk, pk) = generate_keypair();
    let (tx, _rx) = mpsc::channel();
    let router = Arc::new(SimRouter::new(EngineChannel::Local(tx)));
    AppState::new(
        router,
        1,
        sk,
        pk,
        "token".into(),
        None,
        None,
        None,
        None,
    )
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "snake_case")]
enum TestStatus {
    Pending,
    Applied,
    TimedOut,
    RolledBack,
}

#[derive(Debug, Deserialize)]
struct OperationResp {
    op_id: u64,
    status: TestStatus,
    target_max_fee: Option<u64>,
    target_congestion_threshold: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ConfigStateResp {
    current_max_fee: Option<u64>,
    current_congestion_threshold: Option<f64>,
    pending: Option<OperationResp>,
    history: Vec<OperationResp>,
}

fn router_with_state(state: AppState) -> Router {
    server::router(state)
}

async fn post_config(router: &Router, body: Value) -> (StatusCode, OperationResp) {
    let response = router
        .clone()
        .oneshot(
            Request::post("/watcher-config")
                .header("content-type", "application/json")
                .header("x-api-key", "token")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("watcher config response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body bytes")
        .to_bytes();
    let op = serde_json::from_slice::<OperationResp>(&bytes).expect("operation json");
    (status, op)
}

async fn get_state(router: &Router) -> ConfigStateResp {
    let response = router
        .clone()
        .oneshot(
            Request::get("/watcher-config")
                .header("x-api-key", "token")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("watcher state response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body bytes")
        .to_bytes();
    serde_json::from_slice::<ConfigStateResp>(&bytes).expect("state json")
}

async fn post_rollback(router: &Router, op_id: u64) -> StatusCode {
    router
        .clone()
        .oneshot(
            Request::post(format!("/watcher-config/{op_id}/rollback"))
                .header("x-api-key", "token")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("rollback response")
        .status()
}

#[tokio::test]
async fn watcher_config_applied_on_success() {
    let _g = test_guard().await;
    init_policy(MIN_FEE, 1.0);
    let state = test_state();
    let router = router_with_state(state);

    let desired_fee = MIN_FEE + 5_000;
    let desired_threshold = 0.4;
    let (status, op) = post_config(
        &router,
        serde_json::json!({
            "max_fee": desired_fee,
            "congestion_threshold": desired_threshold,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(op.status, TestStatus::Pending);

    init_policy(desired_fee, desired_threshold);
    tokio::time::sleep(Duration::from_millis(300)).await;

    let state = get_state(&router).await;
    assert!(state.pending.is_none());
    let last = state.history.last().expect("history entry");
    assert_eq!(last.status, TestStatus::Applied);
    assert_eq!(last.target_max_fee, Some(desired_fee));
    assert_eq!(last.target_congestion_threshold, Some(desired_threshold));
    assert_eq!(state.current_congestion_threshold, Some(desired_threshold));
}

#[tokio::test]
async fn watcher_config_times_out_without_metrics() {
    let _g = test_guard().await;
    init_policy(MIN_FEE, 1.0);
    let router = router_with_state(test_state());

    let (status, op) = post_config(&router, serde_json::json!({"max_fee": MIN_FEE + 10_000})).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(op.status, TestStatus::Pending);

    tokio::time::sleep(Duration::from_millis(2500)).await;

    let state = get_state(&router).await;
    let pending = state.pending.expect("pending op");
    assert_eq!(pending.status, TestStatus::TimedOut);
    assert_eq!(pending.op_id, op.op_id);
    assert!(pending.target_congestion_threshold.is_none());
    assert_eq!(state.current_congestion_threshold, Some(1.0));
}

#[tokio::test]
async fn watcher_config_manual_revert_clears_timeout() {
    let _g = test_guard().await;
    init_policy(MIN_FEE, 1.0);
    let router = router_with_state(test_state());

    let (status, op) = post_config(&router, serde_json::json!({
        "max_fee": MIN_FEE + 20_000,
        "congestion_threshold": 0.2,
    }))
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    tokio::time::sleep(Duration::from_millis(2500)).await;
    let state = get_state(&router).await;
    assert_eq!(state.pending.as_ref().map(|p| p.status.clone()), Some(TestStatus::TimedOut));
    assert_eq!(state.pending.as_ref().map(|p| p.op_id), Some(op.op_id));
    assert_eq!(
        state.pending.as_ref().and_then(|p| p.target_congestion_threshold),
        Some(0.2)
    );

    let rollback_status = post_rollback(&router, op.op_id).await;
    assert_eq!(rollback_status, StatusCode::OK);

    let state = get_state(&router).await;
    assert!(state.pending.is_none());
    let last = state.history.last().expect("history entry");
    assert_eq!(last.status, TestStatus::RolledBack);
    assert!(state.current_max_fee.is_none());
    assert!(state.current_congestion_threshold.is_none());
}
