use crate::models::{Invoice, Mempool, GuardianMetrics, Webhook};
use reqwest::Client;
use serde_json::{json, Value};
use ratatui::style::Color;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Actions,
    Invoices,
    Watcher,
    Guardian,
    Webhooks,
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
    pub webhooks: Vec<Webhook>,
    pub focus: Focus,
    pub selection: usize,
    pub status: Option<StatusMessage>,
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
            webhooks: Vec::new(),
            focus: Focus::Actions,
            selection: 0,
            status: None,
            client: Client::new(),
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
        self.invoices.get(self.selection).and_then(|inv| {
            inv.get("id")
                .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        })
    }

    pub async fn create_invoice(&mut self, amount_sompi: u64, memo: String) {
        let body = json!({ "amount_sompi": amount_sompi, "memo": memo });
        match self
            .client
            .post(format!("{}/invoice", self.merchant_url))
            .json(&body)
            .send()
            .await
        {
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
            match self
                .client
                .post(format!("{}/pay", self.merchant_url))
                .json(&body)
                .send()
                .await
            {
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
            match self
                .client
                .post(format!("{}/ack", self.merchant_url))
                .json(&body)
                .send()
                .await
            {
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
}

