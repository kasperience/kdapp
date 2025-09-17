use crate::{
    app::{ApiKeyModal, App, ConfigOpStatus, Focus, ListMode, WatcherConfigModal, WatcherField},
    logo,
    models::{invoice_to_string, subscription_to_string},
};
use rand::Rng;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex as AsyncMutex, Notify};

pub struct RefreshScheduler {
    interval: Duration,
    jitter: Duration,
    notify: Notify,
    pending: AtomicBool,
    refresh_lock: AsyncMutex<()>,
    last_refresh: Mutex<Option<Instant>>,
}

impl RefreshScheduler {
    pub fn new(interval: Duration, jitter: Duration) -> Self {
        Self {
            interval,
            jitter,
            notify: Notify::new(),
            pending: AtomicBool::new(false),
            refresh_lock: AsyncMutex::new(()),
            last_refresh: Mutex::new(None),
        }
    }

    fn next_delay(&self) -> Duration {
        if self.interval.is_zero() {
            return Duration::ZERO;
        }
        let jitter_ms = self.jitter.as_millis().min(u128::from(u64::MAX)) as u64;
        let extra = if jitter_ms == 0 {
            Duration::ZERO
        } else {
            let ms = rand::thread_rng().gen_range(0..=jitter_ms);
            Duration::from_millis(ms)
        };
        self.interval.saturating_add(extra)
    }

    fn consume_pending(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }

    #[allow(dead_code)]
    pub fn request_refresh(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            self.notify.notify_one();
        }
    }

    pub async fn refresh_now(&self, app: Arc<AsyncMutex<App>>) {
        let _guard = self.refresh_lock.lock().await;
        App::refresh_task(app.clone()).await;
        let mut last = self.last_refresh.lock().unwrap();
        *last = Some(Instant::now());
    }

    pub async fn run(self: Arc<Self>, app: Arc<AsyncMutex<App>>) {
        loop {
            if self.consume_pending() {
                self.refresh_now(app.clone()).await;
                continue;
            }
            let sleep = tokio::time::sleep(self.next_delay());
            tokio::pin!(sleep);
            let notified = self.notify.notified();
            tokio::pin!(notified);

            tokio::select! {
                _ = &mut sleep => {},
                _ = &mut notified => {
                    continue;
                }
            }
            self.refresh_now(app.clone()).await;
        }
    }

    #[allow(dead_code)]
    pub fn last_refresh(&self) -> Option<Instant> {
        *self.last_refresh.lock().unwrap()
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let size = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            // Make Actions tall enough to show all shortcuts
            Constraint::Length(12),
            Constraint::Min(3),
            Constraint::Min(3),
            Constraint::Min(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(size);

    let header = logo::render_logo();
    f.render_widget(header, chunks[0]);

    render_actions(f, app, chunks[1]);
    render_items(f, app, chunks[2]);
    render_watcher(f, app, chunks[3]);
    render_guardian(f, app, chunks[4]);
    render_webhooks(f, app, chunks[5]);

    let (text, color) = if let Some(status) = &app.status {
        (status.msg.clone(), status.color)
    } else {
        (format!("focus: {:?}", app.focus), Color::White)
    };
    let status = Paragraph::new(text).style(Style::default().fg(color));
    f.render_widget(status, chunks[6]);

    if let Some(modal) = &app.watcher_config {
        render_watcher_modal(f, modal);
    }
    if let Some(modal) = &app.api_key_modal {
        render_api_key_modal(f, modal);
    }
}

fn panel_block(title: &str, focused: bool) -> Block {
    let mut block = Block::default().title(title).borders(Borders::ALL);
    if focused {
        block = block.border_style(Style::default().fg(Color::Yellow));
    }
    block
}

fn render_actions(f: &mut Frame, app: &App, area: Rect) {
    let block = panel_block("Actions", app.focus == Focus::Actions);
    let mut items = vec![Line::raw("q: quit"), Line::raw("r: refresh"), Line::raw("tab: toggle list"), Line::raw("n: new invoice")];
    if app.mock_l1 {
        items.push(Line::raw("p: simulate pay"));
    }
    items.extend([
        Line::raw("a: acknowledge"),
        Line::raw("d: dispute"),
        Line::raw("s: charge sub"),
        Line::raw("w: watcher config"),
        Line::raw("left/right: change focus"),
        Line::raw("up/down: move selection"),
    ]);
    if app.timed_out_config_id().is_some() {
        items.push(Line::raw("x: rollback watcher config"));
    }
    f.render_widget(Paragraph::new(items).block(block), area);
}

fn render_items(f: &mut Frame, app: &App, area: Rect) {
    let title = match app.list_mode {
        ListMode::Invoices => "Invoices",
        ListMode::Subscriptions => "Subscriptions",
    };
    let block = panel_block(title, app.focus == Focus::Invoices);
    let items: Vec<ListItem> = match app.list_mode {
        ListMode::Invoices => app.invoices.iter().map(|i| ListItem::new(invoice_to_string(i))).collect(),
        ListMode::Subscriptions => app.subscriptions.iter().map(|s| ListItem::new(subscription_to_string(s))).collect(),
    };
    let mut state = ListState::default();
    state.select(Some(app.selection));
    let list = List::new(items).block(block).highlight_style(Style::default().bg(Color::Blue));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_watcher(f: &mut Frame, app: &App, area: Rect) {
    let block = panel_block("Watcher", app.focus == Focus::Watcher);
    let mut lines = Vec::new();
    if let Some(obj) = app.watcher.as_object() {
        if let Some(err) = obj.get("error").and_then(|v| v.as_str()) {
            lines.push(err.to_string());
        } else if let (Some(base), Some(cong)) =
            (obj.get("est_base_fee").and_then(|v| v.as_u64()), obj.get("congestion_ratio").and_then(|v| v.as_f64()))
        {
            let min = obj.get("min").and_then(|v| v.as_u64()).unwrap_or(0);
            let max = obj.get("max").and_then(|v| v.as_u64()).unwrap_or(0);
            let policy = obj.get("policy").and_then(|v| v.as_str()).unwrap_or("");
            lines.push(format!("est_base_fee: {base}"));
            lines.push(format!("congestion_ratio: {cong:.2}"));
            lines.push(format!("min: {min} max: {max}"));
            if let Some(th) = obj.get("congestion_threshold").and_then(|v| v.as_f64()) {
                lines.push(format!("threshold: {th:.2}"));
            }
            lines.push(format!("policy: {policy}"));
        }
    }
    if lines.is_empty() {
        lines.push("metrics unavailable".to_string());
    }
    if let Some(max) = app.watcher_state.current_max_fee {
        lines.push(format!("override max_fee: {max}"));
    }
    if let Some(th) = app.watcher_state.current_congestion_threshold {
        lines.push(format!("override threshold: {th:.2}"));
    }
    if let Some(op) = app.watcher_state.pending.as_ref() {
        lines.push(format!("config #{}, status: {}", op.id, config_status_label(op.status)));
    } else if let Some(last) = app.watcher_state.history.last() {
        lines.push(format!("last config #{}, status: {}", last.id, config_status_label(last.status)));
    }
    let paragraph = Paragraph::new(lines.join("\n")).block(block);
    f.render_widget(paragraph, area);
}

fn render_guardian(f: &mut Frame, app: &App, area: Rect) {
    let block = panel_block("Guardian", app.focus == Focus::Guardian);
    let text = if let Some(obj) = app.guardian.as_object() {
        let disputes = obj.get("disputes_open").and_then(|v| v.as_i64()).unwrap_or(0);
        let refunds = obj.get("refunds_signed").and_then(|v| v.as_i64()).unwrap_or(0);
        format!("disputes_open: {disputes}\nrefunds_signed: {refunds}")
    } else {
        app.guardian.to_string()
    };
    f.render_widget(Paragraph::new(text).block(block), area);
}

fn render_webhooks(f: &mut Frame, app: &App, area: Rect) {
    let block = panel_block("Webhooks", app.focus == Focus::Webhooks);
    if app.webhooks.is_empty() {
        f.render_widget(Paragraph::new("None").block(block), area);
    } else {
        let items: Vec<ListItem> =
            app.webhooks.iter().map(|w| ListItem::new(format!("{} id={} ts={} {}", w.event, w.id, w.ts, w.details))).collect();
        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }
}

fn render_watcher_modal(f: &mut Frame, modal: &WatcherConfigModal) {
    let area = centered_rect(60, 40, f.size());
    f.render_widget(Clear, area);
    let block = Block::default().title("Watcher Config").borders(Borders::ALL);
    let mode_line = Line::raw(format!("Mode: {}", modal.mode.as_str()));
    let max_style = if modal.field == WatcherField::MaxFee { Style::default().fg(Color::Yellow) } else { Style::default() };
    let th_style =
        if modal.field == WatcherField::CongestionThreshold { Style::default().fg(Color::Yellow) } else { Style::default() };
    let max_line = Line::styled(format!("max_fee: {}", modal.max_fee), max_style);
    let th_line = Line::styled(format!("congestion_threshold: {}", modal.congestion_threshold), th_style);
    let buttons = Line::raw("[Apply] [Cancel]");
    let paragraph = Paragraph::new(vec![mode_line, max_line, th_line, buttons]).block(block);
    f.render_widget(paragraph, area);
}

fn render_api_key_modal(f: &mut Frame, modal: &ApiKeyModal) {
    let area = centered_rect(60, 30, f.size());
    f.render_widget(Clear, area);
    let block = Block::default().title("Enter API Key").borders(Borders::ALL);
    let hint = Line::raw("Enter the merchant API key (Esc to cancel)");
    let input = Line::raw(modal.value.to_string());
    let paragraph = Paragraph::new(vec![hint, input]).block(block);
    f.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn config_status_label(status: ConfigOpStatus) -> &'static str {
    match status {
        ConfigOpStatus::Pending => "pending",
        ConfigOpStatus::Applied => "applied",
        ConfigOpStatus::TimedOut => "timed out",
        ConfigOpStatus::RolledBack => "rolled back",
    }
}
