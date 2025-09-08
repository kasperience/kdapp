use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use kdapp::episode::{EpisodeEventHandler, EpisodeId, PayloadMetadata};
use kdapp::pki::PubKey;

use crate::client_sender;
use crate::episode::{MerchantCommand, ReceiptEpisode};
use crate::storage;
use crate::tlv::{hash_state, MsgType, TlvMsg, DEMO_HMAC_KEY, TLV_VERSION};
use kdapp_guardian::{self as guardian};

pub struct MerchantEventHandler;

const WATCHER_ADDR: &str = "127.0.0.1:9590";
const CHECKPOINT_INTERVAL_SECS: u64 = 60;

static SEQS: OnceLock<Mutex<HashMap<EpisodeId, u64>>> = OnceLock::new();
static LAST_CKPT: OnceLock<Mutex<HashMap<EpisodeId, u64>>> = OnceLock::new();
static DID_HANDSHAKE: OnceLock<()> = OnceLock::new();
static GUARDIAN: OnceLock<(String, PubKey)> = OnceLock::new();
static GUARDIAN_HANDSHAKE: OnceLock<()> = OnceLock::new();

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn set_guardian(addr: String, pk: PubKey) {
    let _ = GUARDIAN.set((addr, pk));
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
        if let Some((addr, gpk)) = GUARDIAN.get() {
            GUARDIAN_HANDSHAKE.get_or_init(|| {
                if let Some(mpk) = episode.merchant_keys.first() {
                    guardian::handshake(addr, *mpk, *gpk, guardian::DEMO_HMAC_KEY);
                }
            });
            guardian::send_confirm(addr, episode_id as u64, *seq, guardian::DEMO_HMAC_KEY);
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
    }

    fn on_rollback(&self, episode_id: EpisodeId, _episode: &ReceiptEpisode) {
        log::warn!("episode {episode_id}: rolled back last command");
    }
}
