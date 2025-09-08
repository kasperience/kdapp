use std::net::UdpSocket;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::client::KaspaRpcClient;
use kdapp::proxy;
use log::{info, warn};

use crate::{receive, GuardianMsg, GuardianState, DEMO_HMAC_KEY};

#[derive(Clone, Debug)]
struct OkcpRecord {
    episode_id: u64,
    seq: u64,
}

fn decode_okcp(bytes: &[u8]) -> Option<OkcpRecord> {
    const MIN_LEN: usize = 4 + 1 + 8 + 8 + 32;
    if bytes.len() < MIN_LEN {
        return None;
    }
    if &bytes[0..4] != b"OKCP" || bytes[4] != 1 {
        return None;
    }
    let pid_start = 5;
    let pid_end = pid_start + 8;
    let seq_end = pid_end + 8;
    let episode_id = u64::from_le_bytes(bytes[pid_start..pid_end].try_into().ok()?);
    let seq = u64::from_le_bytes(bytes[pid_end..seq_end].try_into().ok()?);
    Some(OkcpRecord { episode_id, seq })
}

async fn watch_anchors(client: &KaspaRpcClient, state: Arc<Mutex<GuardianState>>) -> Result<(), Box<dyn std::error::Error>> {
    // Poll the virtual chain and scan merged blocks for OKCP payloads
    use tokio::time::{sleep, Duration};

    let info = client.get_block_dag_info().await?;
    let mut sink = info.sink;
    loop {
        let vcb = match client.get_virtual_chain_from_block(sink, true).await {
            Ok(v) => v,
            Err(e) => {
                // Try to reconnect and continue
                let _ = client.connect(Some(kdapp::proxy::connect_options())).await;
                sleep(Duration::from_millis(500)).await;
                continue;
            }
        };

        if let Some(new_sink) = vcb.accepted_transaction_ids.last().map(|ncb| ncb.accepting_block_hash) {
            sink = new_sink;
        } else {
            sleep(Duration::from_millis(500)).await;
            continue;
        }

        for ncb in vcb.accepted_transaction_ids {
            let accepting_hash = ncb.accepting_block_hash;
            let accepting_block = match client.get_block(accepting_hash, false).await {
                Ok(b) => b,
                Err(_) => continue,
            };
            let Some(verbose) = accepting_block.verbose_data else { continue };
            // Iterate merged blocks and inspect transactions for OKCP payloads
            for merged_hash in verbose
                .merge_set_blues_hashes
                .into_iter()
                .chain(verbose.merge_set_reds_hashes.into_iter())
            {
                let merged = match client.get_block(merged_hash, true).await {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                for tx in merged.transactions {
                    let payload = &tx.payload;
                    if !payload.is_empty() {
                        if let Some(rec) = decode_okcp(payload) {
                            let mut s = state.lock().unwrap();
                            s.record_checkpoint(rec.episode_id, rec.seq);
                        }
                    }
                }
            }
        }
    }
}

fn handle_escalate(state: &Arc<Mutex<GuardianState>>, episode_id: u64) {
    let known = {
        let s = state.lock().unwrap();
        s.checkpoints.iter().any(|(id, _)| *id == episode_id)
    };
    if known {
        info!("guardian: verified episode {episode_id}; co-signing transaction");
        // Placeholder for co-signing refund/release transactions
    } else {
        warn!("guardian: escalation for unknown episode {episode_id}");
    }
}

static STARTED: OnceLock<()> = OnceLock::new();

pub fn run(bind: &str, wrpc_url: Option<String>) -> Arc<Mutex<GuardianState>> {
    STARTED.get_or_init(|| {});
    let sock = UdpSocket::bind(bind).expect("bind guardian service");
    info!("guardian service listening on {bind}");
    let state = Arc::new(Mutex::new(GuardianState::default()));

    // spawn UDP listener
    let sock_clone = sock.try_clone().expect("clone socket");
    let state_clone = state.clone();
    thread::spawn(move || loop {
        let mut st = state_clone.lock().unwrap();
        if let Some(msg) = receive(&sock_clone, &mut st, DEMO_HMAC_KEY) {
            if let GuardianMsg::Escalate { episode_id, .. } = msg {
                drop(st);
                handle_escalate(&state_clone, episode_id);
            }
        }
    });

    // spawn wRPC watcher
    let state_watch = state.clone();
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(async move {
            let network = NetworkId::with_suffix(NetworkType::Testnet, 10);
            if let Ok(client) = proxy::connect_client(network, wrpc_url).await {
                let _ = watch_anchors(&client, state_watch).await;
            }
        });
    });

    state
}
