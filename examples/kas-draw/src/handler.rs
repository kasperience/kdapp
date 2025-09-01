use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

use kdapp::episode::{EpisodeEventHandler, EpisodeId, PayloadMetadata};
use kdapp::pki::PubKey;

use crate::episode::{LotteryCommand, LotteryEpisode};
use crate::tlv::hash_state;
use crate::watchtower::SimTower;

pub struct Handler {
    tower: Option<SimTower>,
}

impl Handler {
    pub fn new() -> Self {
        // Render an empty dashboard so users see the TUI immediately
        render();
        Self { tower: None }
    }
    pub fn with_tower(tower: SimTower) -> Self {
        // Render an empty dashboard so users see the TUI immediately
        render();
        Self { tower: Some(tower) }
    }
}

#[derive(Clone, Default)]
struct EpisodeSnapshot {
    prize_pool: u64,
    tickets: usize,
    last_winner: Option<u64>,
    paused: bool,
    last_tx: String,
    ticket_price: u64,
    numbers_lo: u8,
    numbers_hi: u8,
    draw_interval_secs: u64,
    next_draw_time: u64,
    authorized_count: usize,
}

#[derive(Default)]
struct Dashboard {
    episodes: HashMap<EpisodeId, EpisodeSnapshot>,
    events: VecDeque<String>,
}

static DASH: OnceLock<Mutex<Dashboard>> = OnceLock::new();
static STATE_NUMS: OnceLock<Mutex<HashMap<EpisodeId, u64>>> = OnceLock::new();

fn dash() -> &'static Mutex<Dashboard> {
    DASH.get_or_init(|| Mutex::new(Dashboard { episodes: HashMap::new(), events: VecDeque::with_capacity(128) }))
}

fn push_event(msg: String) {
    let mut d = dash().lock().unwrap();
    if d.events.len() >= 64 {
        d.events.pop_front();
    }
    d.events.push_back(msg);
}

fn render() {
    let d = dash().lock().unwrap();
    // Clear screen and move cursor home
    print!("\x1b[2J\x1b[H");
    // Fancy ASCII header in teal/cyan for recordings
    println!(
        "\x1b[36;1m\
██╗  ██╗ █████╗ ███████╗      ██████╗ ██████╗  █████╗ ██╗    ██╗\n\
██║ ██╔╝██╔══██╗██╔════╝     ██╔════╝ ██╔══██╗██╔══██╗██║    ██║\n\
█████╔╝ ███████║███████╗     ██║  ███╗██████╔╝███████║██║ █╗ ██║\n\
██╔═██╗ ██╔══██║╚════██║     ██║   ██║██╔══██╗██╔══██║██║███╗██║\n\
██║  ██╗██║  ██║███████║     ╚██████╔╝██║  ██║██║  ██║╚███╔███╔╝\n\
╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝      ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝ ╚══╝╚══╝\x1b[0m\n\
\x1b[36m────────────────────────────────────────────────────────────────────────\x1b[0m\n"
    );
    // Overwrite header with block banner in teal
    {
        let bar = "\u{2588}".repeat(70);
        // Clear again to ensure only the block header shows
        println!("\x1b[2J\x1b[H\x1b[36;1m{}\n\u{2588}{:^68}\u{2588}\n{}\x1b[0m", bar, "KAS DRAW", bar);
    }
    // Mechanics (from the first episode if any)
    if let Some((&id0, snap0)) = d.episodes.iter().min_by_key(|(k, _)| *k) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let mut eta = snap0.next_draw_time.saturating_sub(now);
        if snap0.paused {
            eta = 0;
        }
        let price_kas = (snap0.ticket_price as f64) / 100_000_000.0;
        println!(
            "\x1b[2mMechanics (ep {}): pick 5 unique numbers in [{}..{}], ticket = {:.6} KAS, draw every {}s, next in {}s\x1b[0m\n",
            id0, snap0.numbers_lo, snap0.numbers_hi, price_kas, snap0.draw_interval_secs, eta
        );
    }
    println!("Episodes:");
    println!("  id   | pool (KAS) | tickets | last_winner | paused | last_tx");
    println!("  -----+------------+---------+-------------+--------+--------------------------------");
    let mut keys: Vec<_> = d.episodes.keys().copied().collect();
    keys.sort_unstable();
    for id in keys {
        let e = d.episodes.get(&id).unwrap();
        let pool_kas = (e.prize_pool as f64) / 100_000_000.0;
        println!(
            "  {:>4} | {:>10.6} | {:>7} | {:>11} | {:>6} | {}",
            id,
            pool_kas,
            e.tickets,
            e.last_winner.map(|w| w.to_string()).unwrap_or_else(|| "-".into()),
            if e.paused { "yes" } else { "no" },
            e.last_tx
        );
    }
    println!("\nRecent events:");
    for line in d.events.iter().rev().take(10).rev() {
        println!("  {line}");
    }
    // Ensure output is flushed so it appears immediately
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
}

impl EpisodeEventHandler<LotteryEpisode> for Handler {
    fn on_initialize(&self, episode_id: EpisodeId, episode: &LotteryEpisode) {
        // reset state num for this episode
        STATE_NUMS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap().insert(episode_id, 0);
        let mut d = dash().lock().unwrap();
        d.episodes.insert(
            episode_id,
            EpisodeSnapshot {
                prize_pool: episode.prize_pool,
                tickets: episode.tickets.len(),
                last_winner: episode.last_winner,
                paused: episode.paused,
                last_tx: String::from("-"),
                ticket_price: episode.ticket_price,
                numbers_lo: episode.numbers_range.0,
                numbers_hi: episode.numbers_range.1,
                draw_interval_secs: episode.draw_interval_secs,
                next_draw_time: episode.next_draw_time,
                authorized_count: episode.authorized.len(),
            },
        );
        drop(d);
        // Also show initial state root for easy checkpointing
        let init_bytes = borsh::to_vec(episode).unwrap_or_default();
        let init_hash = hash_state(&init_bytes);
        push_event(format!("ep {} initialized (state_root={})", episode_id, hex::encode(init_hash)));
        render();
    }

    fn on_command(
        &self,
        episode_id: EpisodeId,
        episode: &LotteryEpisode,
        cmd: &LotteryCommand,
        _authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) {
        let mut d = dash().lock().unwrap();
        let entry = d.episodes.entry(episode_id).or_default();
        entry.prize_pool = episode.prize_pool;
        entry.tickets = episode.tickets.len();
        entry.last_winner = episode.last_winner;
        entry.paused = episode.paused;
        entry.last_tx = metadata.tx_id.to_string();
        entry.ticket_price = episode.ticket_price;
        entry.numbers_lo = episode.numbers_range.0;
        entry.numbers_hi = episode.numbers_range.1;
        entry.draw_interval_secs = episode.draw_interval_secs;
        entry.next_draw_time = episode.next_draw_time;
        entry.authorized_count = episode.authorized.len();
        drop(d);

        // Compute state hash and advance local state counter
        let state_bytes = borsh::to_vec(episode).unwrap_or_default();
        let state_hash = hash_state(&state_bytes);
        let mut m = STATE_NUMS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
        let next_num = m.get(&episode_id).copied().unwrap_or(0).saturating_add(1);
        m.insert(episode_id, next_num);
        drop(m);
        if let Some(tower) = &self.tower {
            tower.on_state(episode_id as u64, next_num, state_hash);
            if matches!(cmd, LotteryCommand::CloseEpisode) {
                tower.finalize(episode_id as u64, state_hash);
            }
        }

        match cmd {
            LotteryCommand::ExecuteDraw { .. } => {
                push_event(format!(
                    "ep {} DRAW → last_winner={:?} pool={:.6} KAS",
                    episode_id,
                    episode.last_winner,
                    episode.prize_pool as f64 / 100_000_000.0
                ));
            }
            LotteryCommand::ClaimPrize { ticket_id, .. } => {
                push_event(format!(
                    "ep {} CLAIM ticket={} → pool={:.6} KAS",
                    episode_id,
                    ticket_id,
                    episode.prize_pool as f64 / 100_000_000.0
                ));
            }
            LotteryCommand::BuyTicket { numbers, entry_amount } => {
                push_event(format!(
                    "ep {} BUY {:?} amount={:.6} KAS → pool={:.6} KAS",
                    episode_id,
                    numbers,
                    *entry_amount as f64 / 100_000_000.0,
                    episode.prize_pool as f64 / 100_000_000.0
                ));
            }
            LotteryCommand::CloseEpisode => {
                push_event(format!("ep {episode_id} CLOSE"));
            }
        }
        // Emit current state root to help off-chain checkpointing
        push_event(format!("ep {} STATE state_root={}", episode_id, hex::encode(state_hash)));
        render();
    }

    fn on_rollback(&self, episode_id: EpisodeId, _episode: &LotteryEpisode) {
        push_event(format!("ep {episode_id} ROLLBACK one step"));
        render();
    }
}
