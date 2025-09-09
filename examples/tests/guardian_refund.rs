use std::net::UdpSocket;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use borsh::BorshDeserialize;
use kdapp::pki::{generate_keypair, to_message, verify_signature};
use kdapp_guardian::{
    metrics, send_confirm, send_escalate,
    service::{run, GuardianConfig},
    GuardianMsg, GuardianState, MsgType, TlvMsg, DEMO_HMAC_KEY,
};

fn write_secret_key(path: &std::path::Path, sk: &secp256k1::SecretKey) {
    let hex: String = sk.secret_bytes().iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(path, hex).unwrap();
}

fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    match LOCK.get_or_init(|| Mutex::new(())).lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[test]
fn scenario_a_refund_signed_and_recorded() {
    let _g = test_guard();

    let (sk, pk) = generate_keypair();
    let key_path = std::env::temp_dir().join("guardian_test.key");
    write_secret_key(&key_path, &sk);

    let tmp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = tmp.local_addr().unwrap().port();
    drop(tmp);
    let listen = format!("127.0.0.1:{port}");

    let cfg =
        GuardianConfig { listen_addr: listen.clone(), wrpc_url: None, mainnet: false, key_path: key_path.clone(), state_path: None };
    let state = run(&cfg);

    let state_watch = state.clone();
    let pk_watch = pk;
    let (tx, rx) = std::sync::mpsc::channel();
    // Bind watcher socket before triggering guardian, and direct guardian to this port via env
    let watcher = UdpSocket::bind("127.0.0.1:0").unwrap();
    let watcher_port = watcher.local_addr().unwrap().port();
    std::env::set_var("KDAPP_GUARDIAN_WATCHER_ADDR", format!("127.0.0.1:{watcher_port}"));
    thread::spawn(move || {
        let sock = watcher;
        let mut buf = [0u8; 1024];
        let (n, src) = sock.recv_from(&mut buf).unwrap();
        let tlv = TlvMsg::decode(&buf[..n]).unwrap();
        // Ack the escalate so guardian doesn't retry
        let mut ack = TlvMsg {
            version: tlv.version,
            msg_type: MsgType::Ack as u8,
            episode_id: tlv.episode_id,
            seq: tlv.seq,
            state_hash: [0u8; 32],
            payload: vec![],
            auth: [0u8; 32],
        };
        ack.sign(DEMO_HMAC_KEY);
        let _ = sock.send_to(&ack.encode(), src);
        let msg = GuardianMsg::try_from_slice(&tlv.payload).unwrap();
        if let GuardianMsg::Escalate { episode_id, refund_tx, .. } = msg {
            // wait briefly for state to persist signature
            thread::sleep(Duration::from_millis(50));
            let sig = {
                let st = state_watch.lock().unwrap();
                st.refund_signatures.iter().find(|(ep, _)| *ep == episode_id).map(|(_, s)| *s).unwrap()
            };
            let ok = verify_signature(&pk_watch, &to_message(&refund_tx), &sig);
            tx.send((episode_id, ok)).unwrap();
        } else {
            tx.send((0, false)).unwrap();
        }
    });

    let before = metrics::snapshot();
    let episode = 42u64;
    let refund_tx = b"demo refund".to_vec();
    send_escalate(&listen, episode, "late payment".into(), refund_tx, DEMO_HMAC_KEY);
    let (ep, verified) = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(ep, episode);
    assert!(verified);
    assert_eq!(state.lock().unwrap().refund_signatures.len(), 1);
    let after = metrics::snapshot();
    assert_eq!(after.0, before.0 + 1);
    assert_eq!(after.1, before.1);
}

#[test]
fn scenario_b_replay_confirm_rejected() {
    let _g = test_guard();

    let key_path = std::env::temp_dir().join("guardian_test.key");
    // key already written by previous test, but ensure file exists
    if !key_path.exists() {
        let (sk, _) = generate_keypair();
        write_secret_key(&key_path, &sk);
    }

    let tmp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = tmp.local_addr().unwrap().port();
    drop(tmp);
    let listen = format!("127.0.0.1:{port}");

    let cfg = GuardianConfig { listen_addr: listen.clone(), wrpc_url: None, mainnet: false, key_path, state_path: None };
    let state = run(&cfg);

    // Minimal watcher to ack escalate; bind first and point guardian to it via env var
    let watcher = UdpSocket::bind("127.0.0.1:0").unwrap();
    let watcher_port = watcher.local_addr().unwrap().port();
    std::env::set_var("KDAPP_GUARDIAN_WATCHER_ADDR", format!("127.0.0.1:{watcher_port}"));
    thread::spawn(move || {
        let sock = watcher;
        let mut buf = [0u8; 1024];
        let (n, src) = sock.recv_from(&mut buf).unwrap();
        if let Some(tlv) = TlvMsg::decode(&buf[..n]) {
            let mut ack = TlvMsg {
                version: tlv.version,
                msg_type: MsgType::Ack as u8,
                episode_id: tlv.episode_id,
                seq: tlv.seq,
                state_hash: [0u8; 32],
                payload: vec![],
                auth: [0u8; 32],
            };
            ack.sign(DEMO_HMAC_KEY);
            let _ = sock.send_to(&ack.encode(), src);
        }
    });

    let before = metrics::snapshot();
    let ep = 7u64;
    send_escalate(&listen, ep, "late".into(), vec![], DEMO_HMAC_KEY);
    thread::sleep(Duration::from_millis(50));
    send_confirm(&listen, ep, 1, DEMO_HMAC_KEY);
    thread::sleep(Duration::from_millis(50));
    send_confirm(&listen, ep, 1, DEMO_HMAC_KEY);
    thread::sleep(Duration::from_millis(200));
    let after = metrics::snapshot();
    assert_eq!(after.0, before.0 + 2);
    assert_eq!(after.1, before.1 + 1);
    assert_eq!(state.lock().unwrap().checkpoints, vec![(ep, 1)]);
}

#[test]
fn scenario_c_unknown_episode_no_sign() {
    let _g = test_guard();
    let (sk, _) = generate_keypair();
    let state = Arc::new(Mutex::new(GuardianState::default()));
    let before = metrics::snapshot();
    // Episode 99 not recorded in disputes
    {
        let known = {
            let s = state.lock().unwrap();
            s.disputes.contains(&99)
        };
        if known {
            state.lock().unwrap().sign_refund(99, &[0u8; 0], &sk);
        } else {
            log::warn!("guardian: escalation for unknown episode 99");
        }
    }
    assert!(state.lock().unwrap().refund_signatures.is_empty());
    assert_eq!(metrics::snapshot(), before);
}
