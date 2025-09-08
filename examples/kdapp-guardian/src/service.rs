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

async fn watch_anchors(
    client: &KaspaRpcClient,
    state: Arc<Mutex<GuardianState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use kaspa_rpc_core::notify::virtual_chain_changed::{
        VirtualChainChangedNotification, VirtualChainChangedNotificationType,
    };

    let mut stream = client.subscribe_virtual_chain_changed().await?;
    while let Some(VirtualChainChangedNotification { ty, accepted_blocks, .. }) = stream.recv().await {
        if !matches!(ty, VirtualChainChangedNotificationType::Accepted) {
            continue;
        }
        for block in accepted_blocks {
            for tx in block.transactions {
                if let Some(payload) = tx.payload() {
                    if let Some(rec) = decode_okcp(payload) {
                        let mut s = state.lock().unwrap();
                        s.record_checkpoint(rec.episode_id, rec.seq);
                    }
                }
            }
        }
    }
    Ok(())
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
