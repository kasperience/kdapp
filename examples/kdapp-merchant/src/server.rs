use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::sleep;

use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Router,
};
use kdapp::engine::EpisodeMessage;
use kdapp::pki::PubKey;
use secp256k1::SecretKey;
use serde::{Deserialize, Serialize};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::episode::{MerchantCommand, ReceiptEpisode};
use crate::sim_router::SimRouter;
use crate::storage;
use crate::watcher;

#[derive(Clone, Default)]
pub struct WatcherRuntimeOverrides {
    pub max_fee: Option<u64>,
    pub congestion_threshold: Option<f64>,
}

#[derive(Clone)]
pub struct AppState {
    router: Arc<SimRouter>,
    episode_id: u32,
    merchant_sk: SecretKey,
    merchant_pk: PubKey,
    api_key: String,
    watcher_overrides: Arc<Mutex<WatcherRuntimeOverrides>>,
    webhook_url: Option<String>,
    webhook_secret: Option<Vec<u8>>,
}

impl AppState {
    pub fn new(
        router: Arc<SimRouter>,
        episode_id: u32,
        merchant_sk: SecretKey,
        merchant_pk: PubKey,
        api_key: String,
        max_fee: Option<u64>,
        congestion_threshold: Option<f64>,
        webhook_url: Option<String>,
        webhook_secret: Option<Vec<u8>>,
    ) -> Self {
        let overrides = watcher::WATCHER_OVERRIDES.clone();
        {
            let mut o = overrides.blocking_lock();
            o.max_fee = max_fee;
            o.congestion_threshold = congestion_threshold;
        }
        Self {
            router,
            episode_id,
            merchant_sk,
            merchant_pk,
            api_key,
            watcher_overrides: overrides,
            webhook_url,
            webhook_secret,
        }
    }
}

#[derive(Serialize)]
struct WebhookEvent {
    event: String,
    invoice_id: u64,
    episode_id: u32,
    amount: u64,
    memo: Option<String>,
    payer_pubkey: Option<String>,
    timestamp: u64,
}

fn hmac_sha256(secret: &[u8], message: &str) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("hmac init");
    mac.update(message.as_bytes());
    mac.finalize().into_bytes().into()
}

fn spawn_webhook(state: &AppState, event: WebhookEvent) {
    let (url, secret) = match (state.webhook_url.clone(), state.webhook_secret.clone()) {
        (Some(u), Some(s)) => (u, s),
        _ => return,
    };
    tokio::spawn(async move {
        let body = match serde_json::to_string(&event) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("webhook serialize failed: {e}");
                return;
            }
        };
        let sig = hmac_sha256(&secret, &body);
        let sig_hex = {
            let mut out = vec![0u8; sig.len() * 2];
            faster_hex::hex_encode(&sig, &mut out).expect("hex encode");
            String::from_utf8(out).expect("utf8")
        };
        let client = reqwest::Client::new();
        let mut delay = 1u64;
        for attempt in 0..3 {
            let res = client
                .post(&url)
                .header("X-Signature", sig_hex.clone())
                .body(body.clone())
                .send()
                .await;
            match res {
                Ok(r) if r.status().is_success() => break,
                Ok(r) => log::warn!("webhook POST failed: status {}", r.status()),
                Err(e) => log::warn!("webhook POST failed: {e}"),
            }
            if attempt < 2 {
                sleep(Duration::from_secs(delay)).await;
                delay *= 3;
            }
        }
    });
}

pub async fn serve(bind: String, state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/invoice", post(create_invoice))
        .route("/pay", post(pay_invoice))
        .route("/ack", post(ack_invoice))
        .route("/cancel", post(cancel_invoice))
        .route("/subscribe", post(create_subscription))
        .route("/invoices", get(list_invoices))
        .route("/subscriptions", get(list_subscriptions))
        .route("/watcher-config", post(set_watcher_config))
        .route("/mempool-metrics", get(mempool_metrics))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn authorize(headers: &HeaderMap, state: &AppState) -> Result<(), StatusCode> {
    if let Some(v) = headers.get("x-api-key").and_then(|h| h.to_str().ok()) {
        if v == state.api_key {
            return Ok(());
        }
    }
    Err(StatusCode::UNAUTHORIZED)
}

#[derive(Deserialize)]
struct CreateInvoiceReq {
    invoice_id: u64,
    amount: u64,
    memo: Option<String>,
    guardian_public_keys: Option<Vec<String>>,
}

async fn create_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateInvoiceReq>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let gkeys = req
        .guardian_public_keys
        .unwrap_or_default()
        .iter()
        .filter_map(|h| parse_public_key(h))
        .collect();
    let cmd = MerchantCommand::CreateInvoice {
        invoice_id: req.invoice_id,
        amount: req.amount,
        memo: req.memo.clone(),
        guardian_keys: gkeys,
    };
    let msg = EpisodeMessage::new_signed_command(state.episode_id, cmd, state.merchant_sk, state.merchant_pk);
    if let Err(e) = state.router.forward::<ReceiptEpisode>(msg) {
        log::warn!("forward failed: {e}");
    }
    let event = WebhookEvent {
        event: "invoice_created".into(),
        invoice_id: req.invoice_id,
        episode_id: state.episode_id,
        amount: req.amount,
        memo: req.memo,
        payer_pubkey: None,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    spawn_webhook(&state, event);
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct PayInvoiceReq {
    invoice_id: u64,
    payer_public_key: String,
}

async fn pay_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PayInvoiceReq>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let payer = parse_public_key(&req.payer_public_key).ok_or(StatusCode::BAD_REQUEST)?;
    let cmd = MerchantCommand::MarkPaid { invoice_id: req.invoice_id, payer };
    let msg = EpisodeMessage::<ReceiptEpisode>::UnsignedCommand { episode_id: state.episode_id, cmd };
    if let Err(e) = state.router.forward::<ReceiptEpisode>(msg) {
        log::warn!("forward failed: {e}");
    }
    let (amount, memo) = storage::load_invoices()
        .get(&req.invoice_id)
        .map(|inv| (inv.amount, inv.memo.clone()))
        .unwrap_or((0, None));
    let event = WebhookEvent {
        event: "invoice_paid".into(),
        invoice_id: req.invoice_id,
        episode_id: state.episode_id,
        amount,
        memo,
        payer_pubkey: Some(req.payer_public_key),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    spawn_webhook(&state, event);
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct AckInvoiceReq {
    invoice_id: u64,
}

async fn ack_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AckInvoiceReq>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let cmd = MerchantCommand::AckReceipt { invoice_id: req.invoice_id };
    let msg = EpisodeMessage::new_signed_command(state.episode_id, cmd, state.merchant_sk, state.merchant_pk);
    if let Err(e) = state.router.forward::<ReceiptEpisode>(msg) {
        log::warn!("forward failed: {e}");
    }
    let (amount, memo, payer) = storage::load_invoices()
        .get(&req.invoice_id)
        .map(|inv| (inv.amount, inv.memo.clone(), inv.payer.as_ref().map(pk_to_hex)))
        .unwrap_or((0, None, None));
    let event = WebhookEvent {
        event: "invoice_acked".into(),
        invoice_id: req.invoice_id,
        episode_id: state.episode_id,
        amount,
        memo,
        payer_pubkey: payer,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    spawn_webhook(&state, event);
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct CancelInvoiceReq {
    invoice_id: u64,
}

async fn cancel_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CancelInvoiceReq>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let cmd = MerchantCommand::CancelInvoice { invoice_id: req.invoice_id };
    let msg = EpisodeMessage::<ReceiptEpisode>::UnsignedCommand { episode_id: state.episode_id, cmd };
    if let Err(e) = state.router.forward::<ReceiptEpisode>(msg) {
        log::warn!("forward failed: {e}");
    }
    let (amount, memo, payer) = storage::load_invoices()
        .get(&req.invoice_id)
        .map(|inv| (inv.amount, inv.memo.clone(), inv.payer.as_ref().map(pk_to_hex)))
        .unwrap_or((0, None, None));
    let event = WebhookEvent {
        event: "invoice_cancelled".into(),
        invoice_id: req.invoice_id,
        episode_id: state.episode_id,
        amount,
        memo,
        payer_pubkey: payer,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    spawn_webhook(&state, event);
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct SubscribeReq {
    subscription_id: u64,
    customer_public_key: String,
    amount: u64,
    interval: u64,
}

async fn create_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SubscribeReq>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let customer = parse_public_key(&req.customer_public_key).ok_or(StatusCode::BAD_REQUEST)?;
    let cmd = MerchantCommand::CreateSubscription {
        subscription_id: req.subscription_id,
        customer,
        amount: req.amount,
        interval: req.interval,
    };
    let msg = EpisodeMessage::new_signed_command(state.episode_id, cmd, state.merchant_sk, state.merchant_pk);
    if let Err(e) = state.router.forward::<ReceiptEpisode>(msg) {
        log::warn!("forward failed: {e}");
    }
    Ok(StatusCode::ACCEPTED)
}

#[derive(Serialize)]
struct InvoiceOut {
    id: u64,
    amount: u64,
    memo: Option<String>,
    status: String,
    payer: Option<String>,
    created_at: u64,
    last_update: u64,
}

#[derive(Serialize)]
struct SubscriptionOut {
    id: u64,
    customer: String,
    amount: u64,
    interval: u64,
    next_run: u64,
}

async fn list_invoices(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Vec<InvoiceOut>>, StatusCode> {
    authorize(&headers, &state)?;
    let invoices = storage::load_invoices();
    let out = invoices
        .values()
        .map(|inv| InvoiceOut {
            id: inv.id,
            amount: inv.amount,
            memo: inv.memo.clone(),
            status: format!("{:?}", inv.status),
            payer: inv.payer.as_ref().map(pk_to_hex),
            created_at: inv.created_at,
            last_update: inv.last_update,
        })
        .collect();
    Ok(Json(out))
}

async fn list_subscriptions(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Vec<SubscriptionOut>>, StatusCode> {
    authorize(&headers, &state)?;
    let subs = storage::load_subscriptions();
    let out = subs
        .values()
        .map(|s| SubscriptionOut {
            id: s.id,
            customer: pk_to_hex(&s.customer),
            amount: s.amount,
            interval: s.interval,
            next_run: s.next_run,
        })
        .collect();
    Ok(Json(out))
}

#[derive(Deserialize)]
struct WatcherConfigReq {
    max_fee: Option<u64>,
    congestion_threshold: Option<f64>,
}

async fn set_watcher_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WatcherConfigReq>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let mut o = state.watcher_overrides.lock().await;
    if let Some(fee) = req.max_fee {
        o.max_fee = Some(fee);
    }
    if let Some(th) = req.congestion_threshold {
        o.congestion_threshold = Some(th);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct MempoolMetrics {
    base_fee: u64,
    congestion: f64,
}

async fn mempool_metrics() -> Result<Json<MempoolMetrics>, StatusCode> {
    if let Some(snap) = watcher::get_metrics() {
        Ok(Json(MempoolMetrics { base_fee: snap.est_base_fee, congestion: snap.congestion_ratio }))
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

fn parse_public_key(hex: &str) -> Option<PubKey> {
    let mut buf = [0u8; 33];
    let mut tmp = vec![0u8; hex.len() / 2 + hex.len() % 2];
    if faster_hex::hex_decode(hex.as_bytes(), &mut tmp).is_ok() && tmp.len() == 33 {
        buf.copy_from_slice(&tmp);
        secp256k1::PublicKey::from_slice(&buf).ok().map(PubKey)
    } else {
        None
    }
}

fn pk_to_hex(pk: &PubKey) -> String {
    let bytes = pk.0.serialize();
    let mut out = vec![0u8; bytes.len() * 2];
    faster_hex::hex_encode(&bytes, &mut out).expect("hex encode");
    String::from_utf8(out).expect("utf8")
}
