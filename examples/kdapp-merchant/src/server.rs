use std::sync::Arc;

use axum::{
    extract::{State, Json},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Router,
};
use kdapp::engine::EpisodeMessage;
use kdapp::pki::PubKey;
use secp256k1::SecretKey;
use serde::{Deserialize, Serialize};

use crate::episode::{MerchantCommand, ReceiptEpisode};
use crate::sim_router::SimRouter;
use crate::storage;

#[derive(Clone)]
pub struct AppState {
    router: Arc<SimRouter>,
    episode_id: u32,
    merchant_sk: SecretKey,
    merchant_pk: PubKey,
    api_key: String,
}

impl AppState {
    pub fn new(
        router: Arc<SimRouter>,
        episode_id: u32,
        merchant_sk: SecretKey,
        merchant_pk: PubKey,
        api_key: String,
    ) -> Self {
        Self { router, episode_id, merchant_sk, merchant_pk, api_key }
    }
}

pub async fn serve(bind: String, state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/invoice", post(create_invoice))
        .route("/pay", post(pay_invoice))
        .route("/subscribe", post(create_subscription))
        .route("/invoices", get(list_invoices))
        .route("/subscriptions", get(list_subscriptions))
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
}

async fn create_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateInvoiceReq>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    let cmd = MerchantCommand::CreateInvoice {
        invoice_id: req.invoice_id,
        amount: req.amount,
        memo: req.memo,
    };
    let msg = EpisodeMessage::new_signed_command(state.episode_id, cmd, state.merchant_sk, state.merchant_pk);
    state.router.forward::<ReceiptEpisode>(msg);
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
    let cmd = MerchantCommand::MarkPaid {
        invoice_id: req.invoice_id,
        payer,
    };
    let msg = EpisodeMessage::<ReceiptEpisode>::UnsignedCommand {
        episode_id: state.episode_id,
        cmd,
    };
    state.router.forward::<ReceiptEpisode>(msg);
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
    state.router.forward::<ReceiptEpisode>(msg);
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

async fn list_invoices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<InvoiceOut>>, StatusCode> {
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

async fn list_subscriptions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SubscriptionOut>>, StatusCode> {
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

