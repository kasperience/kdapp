use crate::models::{GuardianMetrics, Invoice, Mempool, Subscription, WebhookEvent};
use ratatui::style::Color;
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Actions,
    Invoices,
    Watcher,
    Guardian,
    Webhooks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMode {
    Invoices,
    Subscriptions,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Default)]
pub struct ApiKeyModal {
    pub value: String,
}

pub struct App {
    pub merchant_url: String,
    pub guardian_url: String,
    pub watcher_url: Option<String>,
    pub api_key: Option<String>,
    pub mock_l1: bool,
    pub invoices: Vec<Invoice>,
    pub subscriptions: Vec<Subscription>,
    pub list_mode: ListMode,
    pub watcher: Mempool,
    pub guardian: GuardianMetrics,
    pub webhooks: VecDeque<WebhookEvent>,
    pub focus: Focus,
    pub selection: usize,
    pub status: Option<StatusMessage>,
    pub watcher_config: Option<WatcherConfigModal>,
    pub api_key_modal: Option<ApiKeyModal>,
    client: Client,
}

impl App {
    pub fn new(
        merchant_url: String,
        guardian_url: String,
        watcher_url: Option<String>,
        api_key: Option<String>,
        mock_l1: bool,
    ) -> Self {
        Self {
            merchant_url,
            guardian_url,
            watcher_url,
            api_key,
            mock_l1,
            invoices: Vec::new(),
            subscriptions: Vec::new(),
            list_mode: ListMode::Invoices,
            watcher: Value::Null,
            guardian: Value::Null,
            webhooks: VecDeque::new(),
            focus: Focus::Actions,
            selection: 0,
            status: None,
            watcher_config: None,
            api_key_modal: None,
            client: Client::new(),
        }
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.merchant_url, path);
        let rb = self.client.get(url);
        if let Some(key) = &self.api_key {
            rb.header("x-api-key", key)
        } else {
            rb
        }
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.merchant_url, path);
        let rb = self.client.post(url);
        if let Some(key) = &self.api_key {
            rb.header("x-api-key", key)
        } else {
            rb
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
            if let Ok(resp) = self.get("/invoices").send().await {
                if resp.status().is_success() {
                    if let Ok(data) = resp.json::<Vec<Invoice>>().await {
                        self.invoices = data;
                    }
                } else if resp.status().as_u16() == 401 {
                    self.set_status("Unauthorized: set API key".into(), Color::Yellow);
                    self.require_api_key_prompt();
                }
            }
            if let Ok(resp) = self.get("/subscriptions").send().await {
                if resp.status().is_success() {
                    if let Ok(data) = resp.json::<Vec<Subscription>>().await {
                        self.subscriptions = data;
                    }
                } else if resp.status().as_u16() == 401 {
                    self.set_status("Unauthorized: set API key".into(), Color::Yellow);
                    self.require_api_key_prompt();
                }
            }
            // Watcher metrics: prefer watcher_url if provided; otherwise fallback to merchant proxy endpoint
            if let Some(url) = &self.watcher_url {
                match self.client.get(format!("{url}/mempool")).send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            if let Ok(data) = resp.json::<Mempool>().await {
                                self.watcher = data;
                            } else {
                                self.watcher = json!({ "error": "unavailable" });
                            }
                        } else {
                            self.watcher = json!({ "error": "unavailable" });
                        }
                    }
                    Err(_) => {
                        self.watcher = json!({ "error": "unavailable" });
                    }
                }
            } else {
                match self.get("/mempool-metrics").send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            if let Ok(data) = resp.json::<Mempool>().await {
                                self.watcher = data;
                            } else {
                                self.watcher = json!({ "error": "unavailable" });
                            }
                        } else if resp.status().as_u16() == 401 {
                            self.set_status("Unauthorized: set API key".into(), Color::Yellow);
                            self.require_api_key_prompt();
                            self.watcher = json!({ "error": "unavailable" });
                        } else {
                            self.watcher = json!({ "error": "unavailable" });
                        }
                    }
                    Err(_) => {
                        self.watcher = json!({ "error": "unavailable" });
                    }
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

    pub fn toggle_list_mode(&mut self) {
        self.list_mode = match self.list_mode {
            ListMode::Invoices => ListMode::Subscriptions,
            ListMode::Subscriptions => ListMode::Invoices,
        };
        self.selection = 0;
    }

    pub fn select_next(&mut self) {
        if let Focus::Invoices = self.focus {
            let len = match self.list_mode {
                ListMode::Invoices => self.invoices.len(),
                ListMode::Subscriptions => self.subscriptions.len(),
            };
            if len > 0 {
                self.selection = (self.selection + 1).min(len.saturating_sub(1));
            }
        }
    }

    pub fn select_prev(&mut self) {
        if let Focus::Invoices = self.focus {
            let len = match self.list_mode {
                ListMode::Invoices => self.invoices.len(),
                ListMode::Subscriptions => self.subscriptions.len(),
            };
            if len > 0 && self.selection > 0 {
                self.selection -= 1;
            }
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

    pub fn selected_invoice_id(&self) -> Option<u64> {
        if let ListMode::Invoices = self.list_mode {
            self.invoices
                .get(self.selection)
                .and_then(|inv| inv.get("id").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))))
        } else {
            None
        }
    }

    pub fn selected_subscription_id(&self) -> Option<u64> {
        if let ListMode::Subscriptions = self.list_mode {
            self.subscriptions.get(self.selection).map(|s| s.id)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub async fn create_invoice(&mut self, amount_sompi: u64, memo: String) {
        // Merchant API expects: { invoice_id, amount, memo }
        // Ask server to assign an ID if 0; otherwise user can provide a specific ID.
        let next_id = self
            .invoices
            .iter()
            .filter_map(|inv| inv.get("id").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))))
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let body = json!({ "invoice_id": next_id, "amount": amount_sompi, "memo": memo });
        match self.post("/invoice").json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                self.set_status("Invoice created".into(), Color::Green);
                self.refresh().await;
            }
            Ok(resp) if resp.status().as_u16() == 401 => {
                self.set_status("Unauthorized: set API key".into(), Color::Red);
                self.require_api_key_prompt();
            }
            Ok(resp) => {
                self.set_status(format!("Error: {}", resp.status()), Color::Red);
            }
            Err(e) => {
                self.set_status(format!("Error: {e}"), Color::Red);
            }
        }
    }

    #[allow(dead_code)]
    pub async fn simulate_payment(&mut self) {
        if !self.mock_l1 {
            self.set_status("Real mode: external L1 payment required.".into(), Color::Yellow);
            return;
        }
        if let Some(id) = self.selected_invoice_id() {
            let body = json!({ "invoice_id": id });
            match self.post("/pay").json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    self.set_status("Payment simulated".into(), Color::Green);
                    self.refresh().await;
                }
                Ok(resp) if resp.status().as_u16() == 401 => {
                    self.set_status("Unauthorized: set API key".into(), Color::Red);
                    self.require_api_key_prompt();
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

    #[allow(dead_code)]
    pub async fn acknowledge_invoice(&mut self) {
        if let Some(id) = self.selected_invoice_id() {
            let body = json!({ "invoice_id": id });
            match self.post("/ack").json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    self.set_status("Invoice acknowledged".into(), Color::Green);
                    self.refresh().await;
                }
                Ok(resp) if resp.status().as_u16() == 401 => {
                    self.set_status("Unauthorized: set API key".into(), Color::Red);
                    self.require_api_key_prompt();
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

    #[allow(dead_code)]
    pub async fn charge_subscription(&mut self) {
        if let Some(id) = self.selected_subscription_id() {
            match self.post(&format!("/subscriptions/{id}/charge")).json(&json!({})).send().await {
                Ok(resp) if resp.status().is_success() => {
                    self.set_status("Subscription charged".into(), Color::Green);
                    self.refresh().await;
                }
                Ok(resp) if resp.status().as_u16() == 401 => {
                    self.set_status("Unauthorized: set API key".into(), Color::Red);
                    self.require_api_key_prompt();
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
        let max_fee = self.watcher.get("max_fee").and_then(|v| v.as_u64()).map(|v| v.to_string()).unwrap_or_default();
        let congestion_threshold =
            self.watcher.get("congestion_threshold").and_then(|v| v.as_f64()).map(|v| v.to_string()).unwrap_or_default();
        let mode = self
            .watcher
            .get("mode")
            .or_else(|| self.watcher.get("policy"))
            .and_then(|v| v.as_str())
            .map(|s| match s.to_lowercase().as_str() {
                "congestion" => WatcherMode::Congestion,
                _ => WatcherMode::Static,
            })
            .unwrap_or(WatcherMode::Static);
        self.watcher_config = Some(WatcherConfigModal { mode, max_fee, congestion_threshold, field: WatcherField::MaxFee });
    }

    pub fn close_watcher_config(&mut self) {
        self.watcher_config = None;
    }

    pub async fn submit_watcher_config(&mut self) {
        if let Some(cfg) = self.watcher_config.take() {
            let current_max = self.watcher.get("max_fee").and_then(|v| v.as_u64()).unwrap_or_default();
            let current_th = self.watcher.get("congestion_threshold").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

            let max_fee = if cfg.max_fee.trim().is_empty() {
                current_max
            } else {
                match cfg.max_fee.parse::<u64>() {
                    Ok(v) => v,
                    Err(_) => {
                        self.set_status("invalid max_fee".into(), Color::Red);
                        self.watcher_config = Some(cfg);
                        return;
                    }
                }
            };

            let th = if cfg.congestion_threshold.trim().is_empty() {
                current_th
            } else {
                match cfg.congestion_threshold.parse::<f32>() {
                    Ok(v) if (0.0..=1.0).contains(&v) => v,
                    Ok(_) | Err(_) => {
                        self.set_status("invalid congestion_threshold".into(), Color::Red);
                        self.watcher_config = Some(cfg);
                        return;
                    }
                }
            };

            let body = json!({
                "mode": match cfg.mode {
                    WatcherMode::Static => "static",
                    WatcherMode::Congestion => "congestion",
                },
                "max_fee": max_fee,
                "congestion_threshold": th,
            });
            match self.post("/watcher-config").json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    self.set_status("Watcher config updated".into(), Color::Green);
                    self.refresh().await;
                }
                Ok(resp) if resp.status().as_u16() == 401 => {
                    self.set_status("Unauthorized: set API key".into(), Color::Red);
                    self.require_api_key_prompt();
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

    pub fn require_api_key_prompt(&mut self) {
        if self.api_key_modal.is_none() {
            self.api_key_modal = Some(ApiKeyModal::default());
        }
    }

    pub fn submit_api_key(&mut self) {
        if let Some(modal) = self.api_key_modal.take() {
            if modal.value.is_empty() {
                self.set_status("API key cannot be empty".into(), Color::Red);
                self.api_key_modal = Some(modal);
            } else {
                self.api_key = Some(modal.value);
                self.set_status("API key set".into(), Color::Green);
            }
        }
    }
    pub fn cancel_api_key_modal(&mut self) {
        self.api_key_modal = None;
    }
}

// ---------- Async tasks that avoid holding the app lock across awaits ----------
fn header_api(rb: reqwest::RequestBuilder, api_key: &Option<String>) -> reqwest::RequestBuilder {
    if let Some(k) = api_key {
        rb.header("x-api-key", k)
    } else {
        rb
    }
}

impl App {
    pub async fn refresh_task(app: Arc<AsyncMutex<App>>) {
        let (client, merchant_url, guardian_url, api_key) = {
            let a = app.lock().await;
            (a.client.clone(), a.merchant_url.clone(), a.guardian_url.clone(), a.api_key.clone())
        };
        if !merchant_url.is_empty() {
            if let Ok(resp) = header_api(client.get(format!("{merchant_url}/invoices")), &api_key).send().await {
                if let Ok(data) = resp.json::<Vec<Invoice>>().await {
                    let mut a = app.lock().await;
                    a.invoices = data;
                }
            }
            if let Ok(resp) = header_api(client.get(format!("{merchant_url}/subscriptions")), &api_key).send().await {
                if let Ok(data) = resp.json::<Vec<Subscription>>().await {
                    let mut a = app.lock().await;
                    a.subscriptions = data;
                }
            }
            if let Ok(resp) = header_api(client.get(format!("{merchant_url}/mempool-metrics")), &api_key).send().await {
                if let Ok(data) = resp.json::<Mempool>().await {
                    let mut a = app.lock().await;
                    a.watcher = data;
                }
            }
        }
        if !guardian_url.is_empty() {
            if let Ok(resp) = client.get(format!("{guardian_url}/metrics")).send().await {
                if let Ok(data) = resp.json::<GuardianMetrics>().await {
                    let mut a = app.lock().await;
                    a.guardian = data;
                }
            }
        }
    }

    async fn poll_invoice_status(app: Arc<AsyncMutex<App>>, invoice_id: u64, target: &str, timeout: Duration) -> bool {
        let start = Instant::now();
        loop {
            App::refresh_task(app.clone()).await;
            {
                let a = app.lock().await;
                let status = a
                    .invoices
                    .iter()
                    .find(|inv| {
                        inv.get("id").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))) == Some(invoice_id)
                    })
                    .and_then(|inv| inv.get("status").and_then(|v| v.as_str()));
                if status == Some(target) {
                    return true;
                }
            }
            if start.elapsed() >= timeout {
                return false;
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    pub async fn create_invoice_task(app: Arc<AsyncMutex<App>>, amount_sompi: u64, memo: String) {
        let (client, merchant_url, api_key) = {
            let a = app.lock().await;
            (a.client.clone(), a.merchant_url.clone(), a.api_key.clone())
        };
        // compute next id from current state
        let next_id = {
            let a = app.lock().await;
            a.invoices
                .iter()
                .filter_map(|inv| inv.get("id").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))))
                .max()
                .unwrap_or(0)
                .saturating_add(1)
        };
        let body = json!({ "invoice_id": next_id, "amount": amount_sompi, "memo": memo });
        match header_api(client.post(format!("{merchant_url}/invoice")), &api_key).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                let mut a = app.lock().await;
                a.set_status("Invoice created".into(), Color::Green);
            }
            Ok(resp) if resp.status().as_u16() == 401 => {
                let mut a = app.lock().await;
                a.set_status("Unauthorized: set API key".into(), Color::Red);
                a.require_api_key_prompt();
            }
            Ok(resp) => {
                let mut a = app.lock().await;
                a.set_status(format!("Error: {}", resp.status()), Color::Red);
            }
            Err(e) => {
                let mut a = app.lock().await;
                a.set_status(format!("Error: {e}"), Color::Red);
            }
        }
        App::refresh_task(app.clone()).await;
    }

    pub async fn ack_task(app: Arc<AsyncMutex<App>>, invoice_id: u64) {
        let (client, merchant_url, api_key) = {
            let a = app.lock().await;
            (a.client.clone(), a.merchant_url.clone(), a.api_key.clone())
        };
        let body = json!({ "invoice_id": invoice_id });
        match header_api(client.post(format!("{merchant_url}/ack")), &api_key).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                {
                    let mut a = app.lock().await;
                    a.set_status("Awaiting acknowledgement".into(), Color::Yellow);
                }
                let acked = App::poll_invoice_status(app.clone(), invoice_id, "Acked", Duration::from_secs(10)).await;
                let mut a = app.lock().await;
                if acked {
                    a.set_status("Invoice acknowledged".into(), Color::Green);
                } else {
                    a.set_status("Acknowledgement pending".into(), Color::Yellow);
                }
            }
            Ok(resp) => {
                let mut a = app.lock().await;
                a.set_status(format!("Error: {}", resp.status()), Color::Red);
            }
            Err(e) => {
                let mut a = app.lock().await;
                a.set_status(format!("Error: {e}"), Color::Red);
            }
        }
        App::refresh_task(app.clone()).await;
    }

    pub async fn dispute_invoice_task(app: Arc<AsyncMutex<App>>, invoice_id: u64) {
        let (client, merchant_url, guardian_url, api_key) = {
            let a = app.lock().await;
            (a.client.clone(), a.merchant_url.clone(), a.guardian_url.clone(), a.api_key.clone())
        };

        let status = {
            let a = app.lock().await;
            a.invoices
                .iter()
                .find(|inv| {
                    inv.get("id").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))) == Some(invoice_id)
                })
                .and_then(|inv| inv.get("status").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string()
        };

        if status != "Acked" {
            if status != "Paid" {
                let mut a = app.lock().await;
                a.set_status("invoice not paid/acked".into(), Color::Yellow);
                return;
            }
            {
                let mut a = app.lock().await;
                a.set_status("Acknowledgement pending".into(), Color::Yellow);
            }
            let _ = App::poll_invoice_status(app.clone(), invoice_id, "Acked", Duration::from_secs(10)).await;
        }

        let current = {
            let a = app.lock().await;
            a.invoices
                .iter()
                .find(|inv| {
                    inv.get("id").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))) == Some(invoice_id)
                })
                .and_then(|inv| inv.get("status").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string()
        };

        if current != "Acked" {
            let mut a = app.lock().await;
            a.set_status("Acknowledgement still pending".into(), Color::Yellow);
            return;
        }

        let body = json!({ "invoice_id": invoice_id, "reason": "demo" });
        let mut success = false;
        if !guardian_url.is_empty() {
            if let Ok(resp) = client.post(format!("{guardian_url}/disputes")).json(&body).send().await {
                if resp.status().is_success() {
                    success = true;
                }
            }
        }
        if !success {
            match header_api(client.post(format!("{merchant_url}/disputes")), &api_key).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    success = true;
                }
                Ok(resp) => {
                    let mut a = app.lock().await;
                    a.set_status(format!("Error: {}", resp.status()), Color::Red);
                    return;
                }
                Err(e) => {
                    let mut a = app.lock().await;
                    a.set_status(format!("Error: {e}"), Color::Red);
                    return;
                }
            }
        }
        {
            let mut a = app.lock().await;
            if success {
                a.set_status("Dispute opened".into(), Color::Green);
            } else {
                a.set_status("Error: dispute failed".into(), Color::Red);
            }
        }
        App::refresh_task(app.clone()).await;
    }

    pub async fn simulate_payment_task(app: Arc<AsyncMutex<App>>, invoice_id: u64) {
        let (client, merchant_url, api_key, mock) = {
            let a = app.lock().await;
            (a.client.clone(), a.merchant_url.clone(), a.api_key.clone(), a.mock_l1)
        };
        if !mock {
            return;
        }
        let body = json!({ "invoice_id": invoice_id });
        match header_api(client.post(format!("{merchant_url}/pay")), &api_key).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                let mut a = app.lock().await;
                a.set_status("Payment simulated".into(), Color::Green);
            }
            Ok(resp) => {
                let mut a = app.lock().await;
                a.set_status(format!("Error: {}", resp.status()), Color::Red);
            }
            Err(e) => {
                let mut a = app.lock().await;
                a.set_status(format!("Error: {e}"), Color::Red);
            }
        }
        App::refresh_task(app.clone()).await;
    }

    pub async fn charge_sub_task(app: Arc<AsyncMutex<App>>, sub_id: u64) {
        let (client, merchant_url, api_key) = {
            let a = app.lock().await;
            (a.client.clone(), a.merchant_url.clone(), a.api_key.clone())
        };
        match header_api(client.post(format!("{merchant_url}/subscriptions/{sub_id}/charge")), &api_key).json(&json!({})).send().await
        {
            Ok(resp) if resp.status().is_success() => {
                let mut a = app.lock().await;
                a.set_status("Subscription charged".into(), Color::Green);
            }
            Ok(resp) => {
                let mut a = app.lock().await;
                a.set_status(format!("Error: {}", resp.status()), Color::Red);
            }
            Err(e) => {
                let mut a = app.lock().await;
                a.set_status(format!("Error: {e}"), Color::Red);
            }
        }
        App::refresh_task(app.clone()).await;
    }

    #[allow(dead_code)]
    pub async fn submit_watcher_config_task(app: Arc<AsyncMutex<App>>, mode: WatcherMode, max_fee: u64, threshold: f32) {
        let (client, merchant_url, api_key) = {
            let a = app.lock().await;
            (a.client.clone(), a.merchant_url.clone(), a.api_key.clone())
        };
        let body = json!({
            "mode": match mode { WatcherMode::Static => "static", WatcherMode::Congestion => "congestion" },
            "max_fee": max_fee,
            "congestion_threshold": threshold,
        });
        match header_api(client.post(format!("{merchant_url}/watcher-config")), &api_key).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                let mut a = app.lock().await;
                a.set_status("Watcher config updated".into(), Color::Green);
            }
            Ok(resp) => {
                let mut a = app.lock().await;
                a.set_status(format!("Error: {}", resp.status()), Color::Red);
            }
            Err(e) => {
                let mut a = app.lock().await;
                a.set_status(format!("Error: {e}"), Color::Red);
            }
        }
        App::refresh_task(app.clone()).await;
    }
}
