use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::sleep;

use axum::{
    extract::{Json, Path, State},
    http::{header, HeaderMap, StatusCode},
    routing::{delete, get, post},
    Router,
};
use hmac::{Hmac, Mac};
use kdapp::engine::EpisodeMessage;
use kdapp::pki::PubKey;
use secp256k1::SecretKey;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::episode::{MerchantCommand, ReceiptEpisode};
use crate::script;
use crate::sim_router::SimRouter;
use crate::storage::{self, ScriptTemplate, TemplateStoreError};
use crate::tlv::Attestation;
use crate::watcher::{self, AttestationSummary};

#[derive(Clone, Default)]
pub struct WatcherRuntimeOverrides {
    pub max_fee: Option<u64>,
    pub congestion_threshold: Option<f64>,
}

const WATCHER_CONFIG_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ConfigOpStatus {
    Pending,
    Applied,
    TimedOut,
    RolledBack,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct ConfigOperation {
    op_id: u64,
    requested_at: u64,
    deadline_at: u64,
    status: ConfigOpStatus,
    target_max_fee: Option<u64>,
    target_congestion_threshold: Option<f64>,
    previous_max_fee: Option<u64>,
    previous_congestion_threshold: Option<f64>,
}

#[derive(Clone)]
struct ConfigOperationInternal {
    op: ConfigOperation,
    deadline: Instant,
}

#[derive(Default, Clone)]
struct ConfigOperations {
    next_id: u64,
    active: Option<ConfigOperationInternal>,
    history: Vec<ConfigOperation>,
}

impl ConfigOperations {
    fn push_history(&mut self, op: ConfigOperation) {
        self.history.push(op);
        if self.history.len() > 10 {
            let drop = self.history.len() - 10;
            self.history.drain(0..drop);
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    router: Arc<SimRouter>,
    episode_id: u32,
    merchant_sk: SecretKey,
    merchant_pk: PubKey,
    api_key: String,
    watcher_overrides: Arc<Mutex<WatcherRuntimeOverrides>>,
    config_ops: Arc<Mutex<ConfigOperations>>,
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
            config_ops: Arc::new(Mutex::new(ConfigOperations::default())),
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
            let res = client.post(&url).header("X-Signature", sig_hex.clone()).body(body.clone()).send().await;
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

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/invoice", post(create_invoice))
        .route("/pay", post(pay_invoice))
        .route("/ack", post(ack_invoice))
        .route("/cancel", post(cancel_invoice))
        .route("/subscribe", post(create_subscription))
        .route("/subscriptions/{sub_id}/charge", post(charge_subscription))
        .route("/subscriptions/{sub_id}/disputes", post(escalate_sub_dispute))
        .route("/invoices", get(list_invoices))
        .route("/subscriptions", get(list_subscriptions))
        .route("/watcher-config", post(set_watcher_config).get(get_watcher_config_status))
        .route("/watcher-config/{op_id}/rollback", post(rollback_watcher_config))
        .route("/mempool-metrics", get(mempool_metrics))
        .route("/attestations", get(list_attestations))
        .route("/attest", post(submit_attestation))
        .route("/policy/templates", post(upsert_policy_template).get(list_policy_templates))
        .route("/policy/templates/{template_id}", delete(remove_policy_template))
        .with_state(state)
}

pub async fn serve(bind: String, state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn authorize(headers: &HeaderMap, state: &AppState) -> Result<(), StatusCode> {
    if let Some(v) = headers.get("x-api-key").and_then(|h| h.to_str().ok()) {
        if !state.api_key.is_empty() && v == state.api_key {
            return Ok(());
        }
    }
    if let Some(token) = session_token_from_headers(headers) {
        if storage::session_token_exists(&token) {
            return Ok(());
        }
    }
    Err(StatusCode::UNAUTHORIZED)
}

fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()) {
        if let Some(rest) = value.strip_prefix("Bearer ") {
            let token = rest.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    if let Some(token) = headers.get("x-session-token").and_then(|h| h.to_str().ok()) {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(cookie) = headers.get(header::COOKIE).and_then(|h| h.to_str().ok()) {
        for part in cookie.split(';') {
            let trimmed = part.trim();
            if let Some(rest) = trimmed.strip_prefix("merchant_session=") {
                if !rest.is_empty() {
                    return Some(rest.to_string());
                }
            }
        }
    }
    None
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
    let gkeys = req.guardian_public_keys.unwrap_or_default().iter().filter_map(|h| parse_public_key(h)).collect();
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
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
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
    let (amount, memo) = storage::load_invoices().get(&req.invoice_id).map(|inv| (inv.amount, inv.memo.clone())).unwrap_or((0, None));
    let event = WebhookEvent {
        event: "invoice_paid".into(),
        invoice_id: req.invoice_id,
        episode_id: state.episode_id,
        amount,
        memo,
        payer_pubkey: Some(req.payer_public_key),
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
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
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
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
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
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

#[derive(Deserialize)]
struct ChargePathReq {}

async fn charge_subscription(
    State(state): State<AppState>,
    Path(sub_id): Path<u64>,
    headers: HeaderMap,
    _body: Json<ChargePathReq>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let cmd = MerchantCommand::ProcessSubscription { subscription_id: sub_id };
    let msg = EpisodeMessage::UnsignedCommand { episode_id: state.episode_id, cmd };
    if let Err(e) = state.router.forward::<ReceiptEpisode>(msg) {
        log::warn!("forward failed: {e}");
    }
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct SubDisputeReq {
    invoice_id: u64,
    reason: String,
    evidence_base64: Option<String>,
}

async fn escalate_sub_dispute(
    State(state): State<AppState>,
    Path(sub_id): Path<u64>,
    headers: HeaderMap,
    Json(req): Json<SubDisputeReq>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let _ = (sub_id, req.invoice_id, req.reason, req.evidence_base64);
    // A real implementation would call ReceiptEpisode::escalate_sub_dispute and send TLVs
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct ScriptTemplateReq {
    template_id: String,
    script_hex: String,
    description: Option<String>,
}

#[derive(Serialize)]
struct ScriptTemplateOut {
    template_id: String,
    script_hex: String,
    description: Option<String>,
}

async fn upsert_policy_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ScriptTemplateReq>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let template_id = req.template_id.trim();
    if template_id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let script_hex = req.script_hex.trim_start_matches("0x");
    let raw_bytes = hex::decode(script_hex).map_err(|_| StatusCode::BAD_REQUEST)?;
    if raw_bytes.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let normalized = script::normalize_script_bytes(&raw_bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
    let template =
        ScriptTemplate { template_id: template_id.to_ascii_lowercase(), script_bytes: normalized, description: req.description };
    match storage::put_script_template(&template) {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(TemplateStoreError::InvalidIdentifier) => Err(StatusCode::BAD_REQUEST),
        Err(TemplateStoreError::EmptyScript) => Err(StatusCode::BAD_REQUEST),
        Err(TemplateStoreError::NotAllowed(_)) => Err(StatusCode::FORBIDDEN),
        Err(TemplateStoreError::Serialize) | Err(TemplateStoreError::Db(_)) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn list_policy_templates(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Vec<ScriptTemplateOut>>, StatusCode> {
    authorize(&headers, &state)?;
    let templates = storage::load_script_templates();
    let mut out = Vec::with_capacity(templates.len());
    for template in templates.values() {
        let mut hex_buf = vec![0u8; template.script_bytes.len() * 2];
        if faster_hex::hex_encode(&template.script_bytes, &mut hex_buf).is_err() {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        let script_hex = String::from_utf8(hex_buf).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        out.push(ScriptTemplateOut {
            template_id: template.template_id.clone(),
            script_hex,
            description: template.description.clone(),
        });
    }
    Ok(Json(out))
}

async fn remove_policy_template(
    State(state): State<AppState>,
    Path(template_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let id = template_id.trim();
    if id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    storage::delete_script_template(&id.to_ascii_lowercase()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
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
    customer_id: u64,
    amount: u64,
    period_secs: u64,
    next_run_ts: u64,
    status: String,
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
            id: s.sub_id,
            customer_id: s.customer_id,
            amount: s.amount_sompi,
            period_secs: s.period_secs,
            next_run_ts: s.next_run_ts,
            status: format!("{:?}", s.status),
        })
        .collect();
    Ok(Json(out))
}

#[derive(Deserialize)]
struct WatcherConfigReq {
    max_fee: Option<u64>,
    congestion_threshold: Option<f64>,
    #[allow(dead_code)]
    mode: Option<String>,
}

#[derive(Serialize)]
struct WatcherConfigStatus {
    current_max_fee: Option<u64>,
    current_congestion_threshold: Option<f64>,
    pending: Option<ConfigOperation>,
    history: Vec<ConfigOperation>,
}

async fn set_watcher_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WatcherConfigReq>,
) -> Result<(StatusCode, Json<ConfigOperation>), StatusCode> {
    authorize(&headers, &state)?;
    if req.max_fee.is_none() && req.congestion_threshold.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    {
        let mut ops = state.config_ops.lock().await;
        if ops.active.is_some() {
            return Err(StatusCode::CONFLICT);
        }
        let mut overrides = state.watcher_overrides.lock().await;
        let prev_max = overrides.max_fee;
        let prev_th = overrides.congestion_threshold;
        if let Some(fee) = req.max_fee {
            overrides.max_fee = Some(fee);
        }
        if let Some(th) = req.congestion_threshold {
            overrides.congestion_threshold = Some(th);
        }
        drop(overrides);
        let now = SystemTime::now();
        let deadline_time = now.checked_add(WATCHER_CONFIG_TIMEOUT).unwrap_or(now);
        let op_id = ops.next_id;
        ops.next_id = ops.next_id.saturating_add(1);
        let op = ConfigOperation {
            op_id,
            requested_at: unix_seconds(now),
            deadline_at: unix_seconds(deadline_time),
            status: ConfigOpStatus::Pending,
            target_max_fee: req.max_fee,
            target_congestion_threshold: req.congestion_threshold,
            previous_max_fee: prev_max,
            previous_congestion_threshold: prev_th,
        };
        let internal = ConfigOperationInternal { op: op.clone(), deadline: Instant::now() + WATCHER_CONFIG_TIMEOUT };
        ops.active = Some(internal);
        drop(ops);
        let monitor_state = state.clone();
        tokio::spawn(async move {
            monitor_config_operation(monitor_state, op_id).await;
        });
        Ok((StatusCode::ACCEPTED, Json(op)))
    }
}

async fn get_watcher_config_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<WatcherConfigStatus>, StatusCode> {
    authorize(&headers, &state)?;
    let overrides = state.watcher_overrides.lock().await.clone();
    let (pending, history) = {
        let ops = state.config_ops.lock().await;
        (ops.active.as_ref().map(|op| op.op.clone()), ops.history.clone())
    };
    Ok(Json(WatcherConfigStatus {
        current_max_fee: overrides.max_fee,
        current_congestion_threshold: overrides.congestion_threshold,
        pending,
        history,
    }))
}

async fn rollback_watcher_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(op_id): Path<u64>,
) -> Result<Json<ConfigOperation>, StatusCode> {
    authorize(&headers, &state)?;
    let (prev_max, prev_th, mut op) = {
        let mut ops = state.config_ops.lock().await;
        let active = ops.active.take().ok_or(StatusCode::NOT_FOUND)?;
        if active.op.op_id != op_id {
            ops.active = Some(active);
            return Err(StatusCode::NOT_FOUND);
        }
        if active.op.status != ConfigOpStatus::TimedOut {
            ops.active = Some(active);
            return Err(StatusCode::BAD_REQUEST);
        }
        let prev_max = active.op.previous_max_fee;
        let prev_th = active.op.previous_congestion_threshold;
        (prev_max, prev_th, active.op)
    };
    {
        let mut overrides = state.watcher_overrides.lock().await;
        overrides.max_fee = prev_max;
        overrides.congestion_threshold = prev_th;
    }
    op.status = ConfigOpStatus::RolledBack;
    {
        let mut ops = state.config_ops.lock().await;
        ops.push_history(op.clone());
    }
    Ok(Json(op))
}

async fn monitor_config_operation(state: AppState, op_id: u64) {
    loop {
        let (deadline, target_max, target_th, status) = {
            let ops = state.config_ops.lock().await;
            let Some(active) = ops.active.as_ref() else { return; };
            if active.op.op_id != op_id {
                return;
            }
            (active.deadline, active.op.target_max_fee, active.op.target_congestion_threshold, active.op.status)
        };
        match status {
            ConfigOpStatus::Pending => {
                if config_operation_applied(target_max, target_th) {
                    let mut ops = state.config_ops.lock().await;
                    if let Some(mut active) = ops.active.take() {
                        if active.op.op_id == op_id && active.op.status == ConfigOpStatus::Pending {
                            active.op.status = ConfigOpStatus::Applied;
                            ops.push_history(active.op);
                        } else {
                            ops.active = Some(active);
                        }
                    }
                    return;
                }
                if Instant::now() >= deadline {
                    let mut ops = state.config_ops.lock().await;
                    if let Some(active) = ops.active.as_mut() {
                        if active.op.op_id == op_id && active.op.status == ConfigOpStatus::Pending {
                            active.op.status = ConfigOpStatus::TimedOut;
                        }
                    }
                    return;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            ConfigOpStatus::TimedOut => return,
            ConfigOpStatus::Applied | ConfigOpStatus::RolledBack => {
                let mut ops = state.config_ops.lock().await;
                if let Some(active) = ops.active.take() {
                    if active.op.op_id == op_id {
                        ops.push_history(active.op);
                    } else {
                        ops.active = Some(active);
                    }
                }
                return;
            }
        }
    }
}

fn config_operation_applied(target_max: Option<u64>, target_threshold: Option<f64>) -> bool {
    let metrics_ok = if let Some(target) = target_max {
        watcher::get_metrics().map_or(false, |snap| snap.max_fee == target)
    } else {
        true
    };
    if !metrics_ok {
        return false;
    }
    if let Some(threshold) = target_threshold {
        let snapshot = watcher::policy_snapshot();
        (snapshot.congestion_threshold - threshold).abs() < f64::EPSILON
    } else {
        true
    }
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
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

async fn list_attestations() -> Json<Vec<AttestationSummary>> {
    Json(watcher::attestation_summaries())
}

async fn submit_attestation(Json(att): Json<Attestation>) -> Result<StatusCode, StatusCode> {
    watcher::ingest_attestation(att).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(StatusCode::ACCEPTED)
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
