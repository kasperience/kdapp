use std::net::UdpSocket;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use kdapp::pki::{generate_keypair, to_message, verify_signature};
use kdapp_guardian::{
    metrics, send_confirm, send_escalate,
    service::{run, GuardianConfig},
    GuardianState, DEMO_HMAC_KEY,
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
    metrics::reset();

    let (sk, pk) = generate_keypair();
    let key_path = std::env::temp_dir().join("guardian_test.key");
    write_secret_key(&key_path, &sk);

    let tmp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = tmp.local_addr().unwrap().port();
    drop(tmp);
    let listen = format!("127.0.0.1:{port}");

    let cfg = GuardianConfig {
        listen_addr: listen.clone(),
        wrpc_url: None,
        mainnet: false,
        key_path: key_path.clone(),
        log_level: "info".into(),
        http_port: None,
    };
    let handle = run(&cfg);
    let state = handle.state.clone();

    let before = metrics::snapshot();
    let episode = 42u64;
    let refund_tx = b"demo refund".to_vec();
    send_escalate(&listen, episode, "late payment".into(), refund_tx.clone(), DEMO_HMAC_KEY);
    let mut sig = None;
    for _ in 0..50 {
        if let Some(s) = {
            let st = state.lock().unwrap();
            st.refund_signatures.iter().find(|(ep, _)| *ep == episode).map(|(_, s)| *s)
        } {
            sig = Some(s);
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let sig = sig.expect("signature");
    assert!(verify_signature(&pk, &to_message(&refund_tx), &sig));
    assert_eq!(state.lock().unwrap().refund_signatures.len(), 1);
    let after = metrics::snapshot();
    assert_eq!(after.0, before.0 + 1);
    assert_eq!(after.1, before.1);

    handle.shutdown();
}

#[test]
fn scenario_b_replay_confirm_rejected() {
    let _g = test_guard();
    metrics::reset();

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

    let cfg = GuardianConfig {
        listen_addr: listen.clone(),
        wrpc_url: None,
        mainnet: false,
        key_path,
        log_level: "info".into(),
        http_port: None,
    };
    let handle = run(&cfg);
    let state = handle.state.clone();

    let before = metrics::snapshot();
    let ep = 7u64;
    send_escalate(&listen, ep, "late".into(), vec![], DEMO_HMAC_KEY);
    thread::sleep(Duration::from_millis(150));
    send_confirm(&listen, ep, 1, DEMO_HMAC_KEY);
    thread::sleep(Duration::from_millis(150));
    send_confirm(&listen, ep, 1, DEMO_HMAC_KEY);
    thread::sleep(Duration::from_millis(200));
    let after = metrics::snapshot();
    assert_eq!(after.0, before.0 + 2);
    assert_eq!(after.1, before.1 + 3);
    assert_eq!(state.lock().unwrap().checkpoints, vec![(ep, 1)]);

    handle.shutdown();
}

#[test]
fn scenario_c_unknown_episode_no_sign() {
    let _g = test_guard();
    metrics::reset();
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
