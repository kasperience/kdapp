use crate::models::{Invoice, Mempool, GuardianMetrics, Webhook};
use reqwest::Client;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Actions,
    Invoices,
    Watcher,
    Guardian,
    Webhooks,
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
}
