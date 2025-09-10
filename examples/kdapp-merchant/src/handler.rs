use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use faster_hex::hex_encode;
use kdapp::episode::{EpisodeEventHandler, EpisodeId, PayloadMetadata};
use kdapp::pki::{to_message, verify_signature, PubKey, Sig};
use hmac::{Hmac, Mac};
use reqwest::blocking::Client;
use serde::Serialize;
use sha2::Sha256;

use crate::client_sender;
use crate::episode::{Invoice, MerchantCommand, ReceiptEpisode};
use crate::storage;
use crate::tlv::{hash_state, MsgType, TlvMsg, DEMO_HMAC_KEY, TLV_VERSION};
use kdapp_guardian::{self as guardian};

pub struct MerchantEventHandler;

const WATCHER_ADDR: &str = "127.0.0.1:9590";
const CHECKPOINT_INTERVAL_SECS: u64 = 60;

static SEQS: OnceLock<Mutex<HashMap<EpisodeId, u64>>> = OnceLock::new();
static LAST_CKPT: OnceLock<Mutex<HashMap<EpisodeId, u64>>> = OnceLock::new();
static DID_HANDSHAKE: OnceLock<()> = OnceLock::new();
static GUARDIANS: OnceLock<Mutex<Vec<(String, PubKey)>>> = OnceLock::new();
static GUARDIAN_HANDSHAKES: OnceLock<Mutex<HashSet<PubKey>>> = OnceLock::new();
static WEBHOOK_URL: OnceLock<Option<String>> = OnceLock::new();
static WEBHOOK_SECRET: OnceLock<Option<String>> = OnceLock::new();

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn add_guardian(addr: String, pk: PubKey) {
    GUARDIANS.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap().push((addr, pk));
}

pub fn set_webhook(url: Option<String>, secret: Option<String>) {
    WEBHOOK_URL.get_or_init(|| url.clone());
    WEBHOOK_SECRET.get_or_init(|| secret);
}

fn pk_to_hex(pk: &PubKey) -> String {
    let bytes = pk.0.serialize();
    let mut out = vec![0u8; bytes.len() * 2];
    hex_encode(&bytes, &mut out).expect("hex encode");
    String::from_utf8(out).expect("utf8")
}

fn emit_checkpoint(episode_id: EpisodeId, episode: &ReceiptEpisode, force: bool) {
    // Ensure a handshake with the watcher before sending signed messages
    DID_HANDSHAKE.get_or_init(|| {
        client_sender::handshake(WATCHER_ADDR, DEMO_HMAC_KEY);
    });
    let now = now();
    let mut last = LAST_CKPT.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
    let should = force || last.get(&episode_id).is_none_or(|t| now.saturating_sub(*t) >= CHECKPOINT_INTERVAL_SECS);
    if !should {
        return;
    }
    last.insert(episode_id, now);
    drop(last);

    if let Ok(bytes) = borsh::to_vec(episode) {
        let state_hash = hash_state(&bytes);
        let mut seqs = SEQS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
        let seq = seqs.entry(episode_id).or_insert(0);
        *seq += 1;
        if let Some(glist) = GUARDIANS.get() {
            let guardians = glist.lock().unwrap().clone();
            let mut handshakes = GUARDIAN_HANDSHAKES.get_or_init(|| Mutex::new(HashSet::new())).lock().unwrap();
            for (addr, gpk) in guardians {
                if episode.guardian_keys.contains(&gpk) {
                    if handshakes.insert(gpk) {
                        if let Some(mpk) = episode.merchant_keys.first() {
                            guardian::handshake(&addr, *mpk, gpk, guardian::DEMO_HMAC_KEY);
                        }
                    }
                    guardian::send_confirm(&addr, episode_id as u64, *seq, guardian::DEMO_HMAC_KEY);
                }
            }
        }
        let msg = TlvMsg {
            version: TLV_VERSION,
            msg_type: MsgType::Checkpoint as u8,
            episode_id: episode_id as u64,
            seq: *seq,
            state_hash,
            payload: vec![],
            auth: [0u8; 32],
        };
        // Sign within the sender using the demo key for now
        client_sender::send_with_retry(WATCHER_ADDR, msg, false, DEMO_HMAC_KEY, true);
        let mut hex = [0u8; 64];
        let _ = faster_hex::hex_encode(&state_hash, &mut hex);
        if let Ok(h) = std::str::from_utf8(&hex) {
            log::info!("checkpoint sent: ep={episode_id} seq={seq} hash={h}");
        }
    }
}

fn forward_dispute(episode_id: EpisodeId, episode: &ReceiptEpisode) {
    if let Some(glist) = GUARDIANS.get() {
        for (addr, gpk) in glist.lock().unwrap().clone() {
            if episode.guardian_keys.contains(&gpk) {
                let refund_tx = b"demo refund".to_vec();
                guardian::send_escalate(
                    &addr,
                    episode_id as u64,
                    "payment dispute".into(),
                    refund_tx.clone(),
                    guardian::DEMO_HMAC_KEY,
                );
                // In a real implementation the guardian's signature would be returned out of band
                // and verified before broadcasting the refund transaction.
                let dummy = secp256k1::ecdsa::Signature::from_compact(&[0u8; 64]);
                if let Ok(sig) = dummy {
                    let sig = Sig(sig);
                    let _ = verify_guardian_cosign(&refund_tx, &sig, &gpk);
                }
            }
        }
    }
}

#[derive(Clone, Serialize)]
struct WebhookEvent {
    invoice_id: u64,
    status: String,
    amount: u64,
    ts: u64,
    merchant_pub: String,
}

fn notify_invoice(inv: &Invoice, merchant: Option<&PubKey>) {
    let url = match WEBHOOK_URL.get().and_then(|u| u.clone()) {
        Some(u) => u,
        None => return,
    };
    let merchant_pub = match merchant {
        Some(pk) => pk_to_hex(pk),
        None => return,
    };
    let event = WebhookEvent {
        invoice_id: inv.id,
        status: format!("{:?}", inv.status),
        amount: inv.amount,
        ts: inv.last_update,
        merchant_pub,
    };
    let secret = WEBHOOK_SECRET.get().and_then(|s| s.clone());
    thread::spawn(move || {
        let client = Client::new();
        let mut delay = Duration::from_secs(1);
        for attempt in 0..3 {
            let body = match serde_json::to_vec(&event) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("webhook serialize error: {e}");
                    return;
                }
            };
            let mut req = client.post(&url).header("Content-Type", "application/json").body(body.clone());
            if let Some(ref key) = secret {
                if let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key.as_bytes()) {
                    mac.update(&body);
                    let sig = mac.finalize().into_bytes();
                    let mut hex = vec![0u8; sig.len() * 2];
                    let _ = faster_hex::hex_encode(&sig, &mut hex);
                    if let Ok(h) = String::from_utf8(hex) {
                        req = req.header("X-Signature", h);
                    }
                }
            }
            match req.send() {
                Ok(resp) if resp.status().is_success() => {
                    log::info!("webhook delivered: status={}", resp.status());
                    break;
                }
                Ok(resp) if resp.status().is_server_error() && attempt < 2 => {
                    log::warn!("webhook server error: status={} attempt={} retrying", resp.status(), attempt + 1);
                }
                Ok(resp) => {
                    log::warn!("webhook failed: status={}", resp.status());
                    break;
                }
                Err(e) if attempt < 2 => {
                    log::warn!("webhook error: {e}; retrying");
                }
                Err(e) => {
                    log::error!("webhook failed: {e}");
                    break;
                }
            }
            thread::sleep(delay);
            delay *= 2;
        }
    });
}

fn verify_guardian_cosign(tx: &[u8], sig: &Sig, gpk: &PubKey) -> bool {
    let msg = to_message(&tx.to_vec());
    verify_signature(gpk, &msg, sig)
}

impl EpisodeEventHandler<ReceiptEpisode> for MerchantEventHandler {
    fn on_initialize(&self, episode_id: EpisodeId, episode: &ReceiptEpisode) {
        log::info!("episode {episode_id} initialized; merchant_keys={:?}", episode.merchant_keys);
        SEQS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap().insert(episode_id, 0);
        emit_checkpoint(episode_id, episode, true);
    }

    fn on_command(
        &self,
        episode_id: EpisodeId,
        episode: &ReceiptEpisode,
        cmd: &MerchantCommand,
        authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) {
        log::info!(
            "episode {episode_id}: cmd={:?}, auth={:?}, tx_id={}, at={}",
            cmd,
            authorization,
            metadata.tx_id,
            metadata.accepting_time
        );
        storage::flush();
        let force = matches!(cmd, MerchantCommand::AckReceipt { .. });
        emit_checkpoint(episode_id, episode, force);
        if matches!(cmd, MerchantCommand::CancelInvoice { .. }) {
            forward_dispute(episode_id, episode);
        }
        if let Some(id) = match cmd {
            MerchantCommand::CreateInvoice { invoice_id, .. } => Some(*invoice_id),
            MerchantCommand::MarkPaid { invoice_id, .. } => Some(*invoice_id),
            MerchantCommand::AckReceipt { invoice_id } => Some(*invoice_id),
            MerchantCommand::CancelInvoice { invoice_id } => Some(*invoice_id),
            _ => None,
        } {
            if let Some(inv) = episode.invoices.get(&id) {
                let merch = episode.merchant_keys.first();
                notify_invoice(inv, merch);
            }
        }
    }

    fn on_rollback(&self, episode_id: EpisodeId, _episode: &ReceiptEpisode) {
        log::warn!("episode {episode_id}: rolled back last command");
    }
}
