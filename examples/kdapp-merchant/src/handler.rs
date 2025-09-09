use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use kdapp::episode::{EpisodeEventHandler, EpisodeId, PayloadMetadata};
use kdapp::pki::PubKey;
use kaspa_addresses::{Address, Prefix as AddrPrefix, Version as AddrVersion};
use kaspa_consensus_core::{
    constants::TX_VERSION,
    sign::sign,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_txscript::pay_to_address_script;
use secp256k1::{Keypair, Secp256k1, SecretKey};

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
static GUARDIANS: OnceLock<Mutex<Vec<(String, PubKey)>>> = OnceLock::new();
static GUARDIAN_HANDSHAKES: OnceLock<Mutex<HashSet<PubKey>>> = OnceLock::new();
static MERCHANT_SK: OnceLock<SecretKey> = OnceLock::new();

pub fn set_merchant_sk(sk: SecretKey) {
    let _ = MERCHANT_SK.set(sk);
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn add_guardian(addr: String, pk: PubKey) {
    GUARDIANS.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap().push((addr, pk));
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

fn create_refund_tx(episode: &ReceiptEpisode, invoice_id: u64) -> Option<Vec<u8>> {
    let sk = MERCHANT_SK.get()?;
    let inv = episode.invoices.get(&invoice_id)?;
    let payer = inv.payer?;
    let txid = inv.carrier_tx?;

    let outpoint = TransactionOutpoint::new(txid, 0);
    let merchant_pk = episode.merchant_keys.first()?.0;
    let addr_prefix = AddrPrefix::Testnet;
    let merchant_addr = Address::new(addr_prefix, AddrVersion::PubKey, &merchant_pk.x_only_public_key().0.serialize());
    let in_script = pay_to_address_script(&merchant_addr);
    let entry = UtxoEntry::new(inv.amount, in_script, 0, false);

    let payer_addr = Address::new(addr_prefix, AddrVersion::PubKey, &payer.0.x_only_public_key().0.serialize());
    let out_script = pay_to_address_script(&payer_addr);
    let fee = 1_000;
    let output = TransactionOutput { value: inv.amount.saturating_sub(fee), script_public_key: out_script };
    let input = TransactionInput { previous_outpoint: outpoint, signature_script: vec![], sequence: 0, sig_op_count: 1 };
    let mut tx = Transaction::new_non_finalized(TX_VERSION, vec![input], vec![output], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    tx.finalize();
    let keypair = Keypair::from_secret_key(&Secp256k1::new(), sk);
    let signed = sign(MutableTransaction::with_entries(tx, vec![entry]), keypair).tx;
    Some(signed.id().as_bytes().to_vec())
}

fn forward_dispute(episode_id: EpisodeId, episode: &ReceiptEpisode, invoice_id: u64) {
    if let Some(refund_tx) = create_refund_tx(episode, invoice_id) {
        if let Some(glist) = GUARDIANS.get() {
            for (addr, gpk) in glist.lock().unwrap().clone() {
                if episode.guardian_keys.contains(&gpk) {
                    guardian::send_escalate(
                        &addr,
                        episode_id as u64,
                        "payment dispute".into(),
                        refund_tx.clone(),
                        guardian::DEMO_HMAC_KEY,
                    );
                }
            }
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
        if let MerchantCommand::CancelInvoice { invoice_id } = cmd {
            forward_dispute(episode_id, episode, *invoice_id);
        }
    }

    fn on_rollback(&self, episode_id: EpisodeId, _episode: &ReceiptEpisode) {
        log::warn!("episode {episode_id}: rolled back last command");
    }
}
