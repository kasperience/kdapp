use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kdapp::engine::EpisodeMessage;

use crate::episode::{MerchantCommand, ReceiptEpisode};
use crate::storage;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

pub fn start(tx: Sender<EpisodeMessage<ReceiptEpisode>>, episode_id: u32) {
    thread::spawn(move || loop {
        let current = now();
        let subs = storage::load_subscriptions();
        for (id, sub) in subs {
            if sub.next_run <= current {
                let cmd = MerchantCommand::ProcessSubscription { subscription_id: id };
                let msg = EpisodeMessage::UnsignedCommand { episode_id, cmd };
                let _ = tx.send(msg);
            }
        }
        thread::sleep(Duration::from_secs(10));
    });
}
