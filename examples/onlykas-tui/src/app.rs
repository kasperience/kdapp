use crate::models::{GuardianMetrics, Invoice, Mempool, WebhookEvent};
use ratatui::style::Color;
use reqwest::Client;
use serde_json::{json, Value};
use std::{collections::VecDeque, time::{Duration, Instant}};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Actions,
    Invoices,
    Watcher,
    Guardian,
    Webhooks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherMode {
    Static,
    Congestion,
}

impl WatcherMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            WatcherMode::Static => "Static",
            WatcherMode::Congestion => "Congestion",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum WatcherField {
    MaxFee,
    CongestionThreshold,
}

pub struct WatcherConfigModal {
    pub mode: WatcherMode,
    pub max_fee: String,
    pub congestion_threshold: String,
    pub field: WatcherField,
}

impl Default for WatcherConfigModal {
    fn default() -> Self {
        Self { mode: WatcherMode::Static, max_fee: String::new(), congestion_threshold: String::new(), field: WatcherField::MaxFee }
    }
}

impl WatcherConfigModal {
    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            WatcherMode::Static => WatcherMode::Congestion,
            WatcherMode::Congestion => WatcherMode::Static,
        };
    }
    pub fn toggle_field(&mut self) {
        self.field = match self.field {
            WatcherField::MaxFee => WatcherField::CongestionThreshold,
            WatcherField::CongestionThreshold => WatcherField::MaxFee,
        };
    }
    pub fn input_char(&mut self, c: char) {
        match self.field {
            WatcherField::MaxFee => {
                if c.is_ascii_digit() {
                    self.max_fee.push(c);
                }
            }
            WatcherField::CongestionThreshold => {
                if c.is_ascii_digit() || (c == '.' && !self.congestion_threshold.contains('.')) {
                    self.congestion_threshold.push(c);
                }
            }
        }
    }
    pub fn backspace(&mut self) {
        match self.field {
            WatcherField::MaxFee => {
                self.max_fee.pop();
            }
            WatcherField::CongestionThreshold => {
                self.congestion_threshold.pop();
            }
        }
    }
}

pub struct StatusMessage {
    pub msg: String,
    pub color: Color,
    time: Instant,
}

pub struct App {
    pub merchant_url: String,
    pub guardian_url: String,
    pub webhook_secret: String,
    pub mock_l1: bool,
    pub invoices: Vec<Invoice>,
    pub watcher: Mempool,
    pub guardian: GuardianMetrics,
    pub webhooks: VecDeque<WebhookEvent>,
    pub focus: Focus,
    pub selection: usize,
    pub status: Option<StatusMessage>,
    pub watcher_config: Option<WatcherConfigModal>,
    client: Client,
}

impl App {
    pub fn new(merchant_url: String, guardian_url: String, webhook_secret: String, mock_l1: bool) -> Self {
        Self {
            merchant_url,
            guardian_url,
            webhook_secret,
            mock_l1,
            invoices: Vec::new(),
            watcher: Value::Null,
            guardian: Value::Null,
            webhooks: VecDeque::new(),
            focus: Focus::Actions,
            selection: 0,
            status: None,
            watcher_config: None,
            client: Client::new(),
        }
    }

    pub fn push_webhook(&mut self, event: WebhookEvent) {
        self.webhooks.push_front(event);
        if self.webhooks.len() > 100 {
            self.webhooks.pop_back();
        }
    }

    pub async fn refresh(&mut self) {
        if !self.merchant_url.is_empty() {
            if let Ok(resp) = self.client.get(format!("{}/invoices", self.merchant_url)).send().await {
                if let Ok(data) = resp.json::<Vec<Invoice>>().await {
                    self.invoices = data;
                }
            }
            if let Ok(resp) = self.client.get(format!("{}/mempool", self.merchant_url)).send().await {
                if let Ok(data) = resp.json::<Mempool>().await {
                    self.watcher = data;
                }
            }
        }
        if !self.guardian_url.is_empty() {
            if let Ok(resp) = self.client.get(format!("{}/metrics", self.guardian_url)).send().await {
                if let Ok(data) = resp.json::<GuardianMetrics>().await {
                    self.guardian = data;
                }
            }
        }
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            Focus::Actions => Focus::Invoices,
            Focus::Invoices => Focus::Watcher,
            Focus::Watcher => Focus::Guardian,
            Focus::Guardian => Focus::Webhooks,
            Focus::Webhooks => Focus::Actions,
        };
        self.selection = 0;
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Focus::Actions => Focus::Webhooks,
            Focus::Invoices => Focus::Actions,
            Focus::Watcher => Focus::Invoices,
            Focus::Guardian => Focus::Watcher,
            Focus::Webhooks => Focus::Guardian,
        };
        self.selection = 0;
    }

    pub fn select_next(&mut self) {
        match self.focus {
            Focus::Invoices => {
                if !self.invoices.is_empty() {
                    self.selection = (self.selection + 1).min(self.invoices.len().saturating_sub(1));
                }
            }
            _ => {}
        }
    }

    pub fn select_prev(&mut self) {
        match self.focus {
            Focus::Invoices => {
                if !self.invoices.is_empty() && self.selection > 0 {
                    self.selection -= 1;
                }
            }
            _ => {}
        }
    }

    pub fn tick(&mut self) {
        if let Some(status) = &self.status {
            if status.time.elapsed() > Duration::from_secs(3) {
                self.status = None;
            }
        }
    }

    pub fn set_status(&mut self, msg: String, color: Color) {
        self.status = Some(StatusMessage { msg, color, time: Instant::now() });
    }

    fn selected_invoice_id(&self) -> Option<u64> {
        self.invoices
            .get(self.selection)
            .and_then(|inv| inv.get("id").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))))
    }

    pub async fn create_invoice(&mut self, amount_sompi: u64, memo: String) {
        let body = json!({ "amount_sompi": amount_sompi, "memo": memo });
        match self.client.post(format!("{}/invoice", self.merchant_url)).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                self.set_status("Invoice created".into(), Color::Green);
                self.refresh().await;
            }
            Ok(resp) => {
                self.set_status(format!("Error: {}", resp.status()), Color::Red);
            }
            Err(e) => {
                self.set_status(format!("Error: {e}"), Color::Red);
            }
        }
    }

    pub async fn simulate_payment(&mut self) {
        if !self.mock_l1 {
            self.set_status("Real mode: external L1 payment required.".into(), Color::Yellow);
            return;
        }
        if let Some(id) = self.selected_invoice_id() {
            let body = json!({ "invoice_id": id });
            match self.client.post(format!("{}/pay", self.merchant_url)).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    self.set_status("Payment simulated".into(), Color::Green);
                    self.refresh().await;
                }
                Ok(resp) => {
                    self.set_status(format!("Error: {}", resp.status()), Color::Red);
                }
                Err(e) => {
                    self.set_status(format!("Error: {e}"), Color::Red);
                }
            }
        }
    }

    pub async fn acknowledge_invoice(&mut self) {
        if let Some(id) = self.selected_invoice_id() {
            let body = json!({ "invoice_id": id });
            match self.client.post(format!("{}/ack", self.merchant_url)).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    self.set_status("Invoice acknowledged".into(), Color::Green);
                    self.refresh().await;
                }
                Ok(resp) => {
                    self.set_status(format!("Error: {}", resp.status()), Color::Red);
                }
                Err(e) => {
                    self.set_status(format!("Error: {e}"), Color::Red);
                }
            }
        }
    }

    pub fn open_watcher_config(&mut self) {
        self.watcher_config = Some(WatcherConfigModal::default());
    }

    pub fn close_watcher_config(&mut self) {
        self.watcher_config = None;
    }

    pub async fn submit_watcher_config(&mut self) {
        if let Some(cfg) = self.watcher_config.take() {
            if let (Ok(max_fee), Ok(th)) = (cfg.max_fee.parse::<u64>(), cfg.congestion_threshold.parse::<f32>()) {
                if th < 0.0 || th > 1.0 {
                    self.set_status("invalid config".into(), Color::Red);
                    self.watcher_config = Some(cfg);
                    return;
                }
                let body = json!({
                    "mode": match cfg.mode {
                        WatcherMode::Static => "static",
                        WatcherMode::Congestion => "congestion",
                    },
                    "max_fee": max_fee,
                    "congestion_threshold": th,
                });
                match self.client.post(format!("{}/watcher-config", self.merchant_url)).json(&body).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        self.set_status("Watcher config updated".into(), Color::Green);
                        self.refresh().await;
                    }
                    Ok(resp) => {
                        self.set_status(format!("Error: {}", resp.status()), Color::Red);
                    }
                    Err(e) => {
                        self.set_status(format!("Error: {e}"), Color::Red);
                    }
                }
            } else {
                self.set_status("invalid config".into(), Color::Red);
                self.watcher_config = Some(cfg);
            }
        }
    }
}
