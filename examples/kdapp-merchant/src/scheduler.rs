use std::collections::HashMap;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::warn;

use kdapp::engine::EpisodeMessage;

use crate::episode::{MerchantCommand, ReceiptEpisode};
use crate::sim_router::SimRouter;
use crate::storage;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_else(|_| Duration::from_secs(0)).as_secs()
}

const INITIAL_BACKOFF: u64 = 5;
const MAX_BACKOFF: u64 = 300;

pub fn start(router: SimRouter, episode_id: u32) {
    thread::spawn(move || {
        let mut backoffs: HashMap<u64, u64> = HashMap::new();
        loop {
            let current = now();
            let subs = storage::load_subscriptions();
            for (id, mut sub) in subs {
                if sub.next_run_ts <= current {
                    let cmd = MerchantCommand::ProcessSubscription { subscription_id: id };
                    let msg = EpisodeMessage::UnsignedCommand { episode_id, cmd };
                    if let Err(e) = router.forward::<ReceiptEpisode>(msg) {
                        let delay = backoffs.get(&id).copied().unwrap_or(INITIAL_BACKOFF);
                        let next_delay = (delay * 2).min(MAX_BACKOFF);
                        backoffs.insert(id, next_delay);
                        sub.next_run_ts = current + delay;
                        storage::put_subscription(&sub);
                        warn!("forward failed for subscription {id}, retrying in {delay}s: {e}");
                    } else {
                        backoffs.remove(&id);
                    }
                }
            }
            thread::sleep(Duration::from_secs(10));
        }
    });
}
