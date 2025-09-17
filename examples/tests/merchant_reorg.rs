use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use kaspa_consensus_core::Hash;
use kdapp::engine::{Engine, EngineMsg, EpisodeMessage};
use kdapp::episode::{EpisodeEventHandler, EpisodeId, PayloadMetadata, TxOutputInfo};
use kdapp::pki::{generate_keypair, PubKey};
use kdapp::proxy::TxStatus;
use kdapp_merchant::episode::{CustomerInfo, InvoiceStatus, MerchantCommand, ReceiptEpisode};
use kdapp_merchant::storage::{self, ConfirmationUpdate};
use tempfile::TempDir;

const MERCHANT_DB_ENV: &str = "MERCHANT_DB_PATH";

#[derive(Clone)]
struct TestMerchantEventHandler {
    events: Arc<Mutex<Vec<TestEvent>>>,
}

impl TestMerchantEventHandler {
    fn new() -> Self {
        Self { events: Arc::new(Mutex::new(Vec::new())) }
    }

    fn events(&self) -> Arc<Mutex<Vec<TestEvent>>> {
        Arc::clone(&self.events)
    }
}

#[derive(Clone, Debug)]
enum TestEvent {
    Paid { confirmations: Option<u64> },
}

impl TestEvent {
    fn confirmations(&self) -> Option<u64> {
        match self {
            TestEvent::Paid { confirmations } => *confirmations,
        }
    }
}

impl EpisodeEventHandler<ReceiptEpisode> for TestMerchantEventHandler {
    fn on_initialize(&self, _episode_id: EpisodeId, _episode: &ReceiptEpisode) {}

    fn on_command(
        &self,
        _episode_id: EpisodeId,
        episode: &ReceiptEpisode,
        cmd: &MerchantCommand,
        _authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) {
        match cmd {
            MerchantCommand::CreateInvoice { invoice_id, .. } => {
                if let Some(inv) = episode.invoices.get(invoice_id) {
                    let _ = storage::persist_invoice_state(inv, ConfirmationUpdate::Clear);
                }
            }
            MerchantCommand::MarkPaid { invoice_id, .. } => {
                let update = metadata
                    .tx_status
                    .as_ref()
                    .map(|status| ConfirmationUpdate::set(metadata.tx_id, status, metadata.accepting_time))
                    .unwrap_or(ConfirmationUpdate::Keep);
                if let Some(inv) = episode.invoices.get(invoice_id) {
                    let _ = storage::persist_invoice_state(inv, update);
                    let confirmations = metadata.tx_status.as_ref().and_then(|s| s.confirmations);
                    self.events.lock().unwrap().push(TestEvent::Paid { confirmations });
                }
            }
            _ => {}
        }
    }

    fn on_rollback(&self, _episode_id: EpisodeId, _episode: &ReceiptEpisode) {}
}

struct EnvGuard {
    key: &'static str,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        std::env::set_var(key, value);
        Self { key }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        std::env::remove_var(self.key);
    }
}

fn hash_from_byte(byte: u8) -> Hash {
    let mut data = [0u8; 32];
    data[31] = byte;
    Hash::from_bytes(data)
}

fn next_tx_hash() -> Hash {
    static COUNTER: AtomicU8 = AtomicU8::new(0);
    let byte = COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_add(100);
    hash_from_byte(byte)
}

type EpisodeEntry = (EpisodeMessage<ReceiptEpisode>, Option<Vec<TxOutputInfo>>, Option<TxStatus>);

fn send_block(
    tx: &Sender<EngineMsg>,
    accepting_hash: Hash,
    accepting_daa: u64,
    accepting_time: u64,
    entries: Vec<EpisodeEntry>,
) {
    let associated_txs = entries
        .into_iter()
        .map(|(msg, outputs, status)| {
            let payload = borsh::to_vec(&msg).expect("serialize episode msg");
            (next_tx_hash(), payload, outputs, status)
        })
        .collect();
    let event = EngineMsg::BlkAccepted { accepting_hash, accepting_daa, accepting_time, associated_txs };
    tx.send(event).expect("send block");
}

fn p2pk_script(pk: &PubKey) -> Vec<u8> {
    let mut script = Vec::with_capacity(35);
    script.push(33);
    script.extend_from_slice(&pk.0.serialize());
    script.push(0xac);
    script
}

fn wait_briefly() {
    thread::sleep(Duration::from_millis(150));
}

#[test]
fn invoice_payment_reorg_resets_confirmations() {
    let temp_dir = TempDir::new().expect("tempdir");
    let db_path = temp_dir.path().join("merchant-reorg-tests.db");
    let path_str = db_path.to_string_lossy().to_string();
    let _env = EnvGuard::set(MERCHANT_DB_ENV, &path_str);

    storage::init();

    let (merchant_sk, merchant_pk) = generate_keypair();
    let (payer_sk, payer_pk) = generate_keypair();
    storage::put_customer(&payer_pk, &CustomerInfo::default());

    let handler = TestMerchantEventHandler::new();
    let events = handler.events();

    let (tx, rx) = mpsc::channel();
    let mut engine: Engine<ReceiptEpisode, TestMerchantEventHandler> = Engine::new(rx);
    let handler_clone = handler.clone();
    let engine_thread = thread::spawn(move || {
        engine.start(vec![handler_clone]);
    });

    let episode_id: EpisodeId = 42;
    let invoice_id = 7u64;

    // Start episode
    send_block(
        &tx,
        hash_from_byte(1),
        1,
        1,
        vec![(EpisodeMessage::NewEpisode { episode_id, participants: vec![merchant_pk] }, None, None)],
    );
    wait_briefly();

    // Merchant creates invoice
    let create_cmd = MerchantCommand::CreateInvoice { invoice_id, amount: 50_000, memo: Some("reorg demo".into()), guardian_keys: vec![] };
    let create_msg = EpisodeMessage::new_signed_command(episode_id, create_cmd, merchant_sk, merchant_pk);
    send_block(&tx, hash_from_byte(2), 2, 2, vec![(create_msg, None, None)]);
    wait_briefly();

    // Payer settles invoice with three confirmations recorded
    let status_high = TxStatus {
        acceptance_height: Some(500),
        confirmations: Some(3),
        finality: Some(false),
    };
    let paid_cmd = MerchantCommand::MarkPaid { invoice_id, payer: payer_pk };
    let paid_msg = EpisodeMessage::new_signed_command(episode_id, paid_cmd, payer_sk, payer_pk);
    let outputs = vec![TxOutputInfo { value: 50_000, script_version: 0, script_bytes: Some(p2pk_script(&merchant_pk)) }];
    send_block(&tx, hash_from_byte(3), 3, 3, vec![(paid_msg, Some(outputs.clone()), Some(status_high))]);
    wait_briefly();

    storage::flush();
    let initial_record = storage::load_invoice_confirmation(invoice_id).expect("initial confirmation");
    assert_eq!(initial_record.status.confirmations, Some(3));
    let invoices = storage::load_invoices();
    assert_eq!(invoices.get(&invoice_id).map(|inv| inv.status.clone()), Some(InvoiceStatus::Paid));
    assert_eq!(events.lock().unwrap().len(), 1);

    // Reorg removes the paying block
    tx.send(EngineMsg::BlkReverted { accepting_hash: hash_from_byte(3) }).expect("send revert");
    wait_briefly();

    storage::flush();
    assert!(storage::load_invoice_confirmation(invoice_id).is_none());
    let invoices = storage::load_invoices();
    assert_eq!(invoices.get(&invoice_id).map(|inv| inv.status.clone()), Some(InvoiceStatus::Open));

    // Re-accept payment on new branch with fewer confirmations
    let status_low = TxStatus {
        acceptance_height: Some(505),
        confirmations: Some(1),
        finality: Some(false),
    };
    let paid_again = EpisodeMessage::new_signed_command(
        episode_id,
        MerchantCommand::MarkPaid { invoice_id, payer: payer_pk },
        payer_sk,
        payer_pk,
    );
    send_block(&tx, hash_from_byte(4), 4, 4, vec![(paid_again, Some(outputs), Some(status_low))]);
    wait_briefly();

    storage::flush();
    let record_after = storage::load_invoice_confirmation(invoice_id).expect("post-reorg confirmation");
    assert_eq!(record_after.status.confirmations, Some(1));
    let invoices = storage::load_invoices();
    assert_eq!(invoices.get(&invoice_id).map(|inv| inv.status.clone()), Some(InvoiceStatus::Paid));

    let events = events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].confirmations(), Some(3));
    assert_eq!(events[1].confirmations(), Some(1));
    drop(events);

    tx.send(EngineMsg::Exit).expect("send exit");
    engine_thread.join().expect("engine thread");
}
