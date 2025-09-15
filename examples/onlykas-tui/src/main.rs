mod actions;
mod app;
mod logo;
mod models;
mod ui;

use actions::Action;
use app::App;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{event, execute};
use hmac::{Hmac, Mac};
use models::WebhookEvent;
use ratatui::{prelude::*, Terminal};
use serde_json::Value;
use sha2::Sha256;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

#[derive(Clone)]
struct WebhookState {
    secret: Vec<u8>,
    tx: mpsc::UnboundedSender<WebhookEvent>,
}

async fn webhook(State(state): State<WebhookState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let sig_hex = match headers.get("X-Signature").and_then(|v| v.to_str().ok()) {
        Some(s) => s,
        None => return StatusCode::UNAUTHORIZED,
    };
    let sig = match hex::decode(sig_hex) {
        Ok(v) => v,
        Err(_) => return StatusCode::UNAUTHORIZED,
    };
    let mut mac = match Hmac::<Sha256>::new_from_slice(&state.secret) {
        Ok(m) => m,
        Err(_) => return StatusCode::UNAUTHORIZED,
    };
    mac.update(&body);
    if mac.verify_slice(&sig).is_err() {
        return StatusCode::UNAUTHORIZED;
    }
    // Accept flexible payloads from merchant; map to TUI's WebhookEvent
    let now_ts = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    match serde_json::from_slice::<Value>(&body) {
        Ok(v) => {
            let (event, id, ts) = if let Some(obj) = v.as_object() {
                let ev = obj.get("event").and_then(|x| x.as_str()).unwrap_or("event").to_string();
                let id = obj
                    .get("invoice_id")
                    .and_then(|x| x.as_u64())
                    .map(|i| i.to_string())
                    .or_else(|| obj.get("id").and_then(|x| x.as_str().map(|s| s.to_string())))
                    .unwrap_or_else(|| "-".into());
                let ts = obj.get("timestamp").and_then(|x| x.as_u64()).unwrap_or_else(now_ts);
                (ev, id, ts)
            } else {
                ("event".into(), "-".into(), now_ts())
            };
            let ev = WebhookEvent { event, id, ts, details: v };
            let _ = state.tx.send(ev);
            StatusCode::OK
        }
        Err(_) => StatusCode::BAD_REQUEST,
    }
}

struct Args {
    merchant_url: String,
    guardian_url: String,
    watcher_url: Option<String>,
    webhook_secret: String,
    api_key: Option<String>,
    webhook_port: Option<u16>,
    mock_l1: bool,
}

fn parse_args() -> Args {
    let mut merchant_url = String::new();
    let mut guardian_url = String::new();
    let mut webhook_secret = String::new();
    let mut watcher_url: Option<String> = None;
    let mut api_key: Option<String> = None;
    let mut mock_l1 = false;
    let mut webhook_port: Option<u16> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--merchant-url" => merchant_url = args.next().unwrap_or_default(),
            "--guardian-url" => guardian_url = args.next().unwrap_or_default(),
            "--watcher-url" => watcher_url = args.next(),
            "--webhook-secret" => webhook_secret = args.next().unwrap_or_default(),
            "--api-key" => api_key = args.next(),
            "--webhook-port" => webhook_port = args.next().and_then(|s| s.parse().ok()),
            "--mock-l1" => mock_l1 = true,
            _ => {}
        }
    }
    let api_key = api_key.map(|s| s.trim().to_string());
    let webhook_secret = webhook_secret.trim().to_string();
    Args { merchant_url, guardian_url, watcher_url, webhook_secret, api_key, webhook_port, mock_l1 }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args();
    let app = Arc::new(Mutex::new(App::new(args.merchant_url, args.guardian_url, args.watcher_url, args.api_key, args.mock_l1)));

    let (tx, mut rx) = mpsc::unbounded_channel();
    {
        let app = Arc::clone(&app);
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let mut app = app.lock().await;
                app.push_webhook(event);
            }
        });
    }

    let secret = hex::decode(args.webhook_secret).unwrap_or_default();
    let state = WebhookState { secret, tx };
    let bind_addr = match args.webhook_port {
        Some(p) => format!("127.0.0.1:{p}"),
        None => "127.0.0.1:0".to_string(),
    };
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    let port = listener.local_addr()?.port();
    {
        let mut app = app.lock().await;
        app.set_status(format!("webhook listening: http://127.0.0.1:{port}/hook"), Color::Green);
    }
    tokio::spawn(async move {
        let router = Router::new().route("/hook", post(webhook)).with_state(state);
        let _ = axum::serve(listener, router).await;
    });

    // background refresh task (no long holds on the mutex)
    {
        let app_clone = Arc::clone(&app);
        tokio::spawn(async move {
            loop {
                App::refresh_task(app_clone.clone()).await;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, event::EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, app.clone()).await;

    // restore terminal
    disable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, event::DisableMouseCapture, LeaveAlternateScreen)?;
    if let Err(e) = res {
        eprintln!("{e:?}");
    }
    Ok(())
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: Arc<Mutex<App>>) -> Result<(), Box<dyn Error>> {
    loop {
        {
            let mut app = app.lock().await;
            app.tick();
            terminal.draw(|f| ui::draw(f, &app))?;
        }
        if event::poll(std::time::Duration::from_millis(50))? {
            if let event::Event::Key(key) = event::read()? {
                // Global shortcuts (work even when a modal is open)
                if matches!(key.code, event::KeyCode::Char('q'))
                    || (matches!(key.code, event::KeyCode::Char('c')) && key.modifiers.contains(event::KeyModifiers::CONTROL))
                {
                    return Ok(());
                }

                // Handle modals first while holding the lock
                let mut handled = false;
                {
                    let mut a = app.lock().await;
                    if let Some(modal) = a.api_key_modal.as_mut() {
                        match key.code {
                            event::KeyCode::Esc => a.cancel_api_key_modal(),
                            event::KeyCode::Enter => {
                                a.submit_api_key();
                                if a.api_key.is_some() {
                                    a.refresh().await;
                                }
                            }
                            event::KeyCode::Backspace => {
                                modal.value.pop();
                            }
                            event::KeyCode::Char(c) => {
                                modal.value.push(c);
                            }
                            _ => {}
                        }
                        handled = true;
                    } else if let Some(modal) = a.watcher_config.as_mut() {
                        match key.code {
                            event::KeyCode::Esc => a.close_watcher_config(),
                            event::KeyCode::Enter => {
                                a.submit_watcher_config().await;
                            }
                            event::KeyCode::Tab => modal.toggle_mode(),
                            event::KeyCode::Up | event::KeyCode::Down => modal.toggle_field(),
                            event::KeyCode::Backspace => modal.backspace(),
                            event::KeyCode::Char(c) => modal.input_char(c),
                            _ => {}
                        }
                        handled = true;
                    }
                }
                if handled {
                    continue;
                }

                // No modal: derive action without lock
                match Action::from_key(key) {
                    Action::Quit => return Ok(()),
                    Action::Refresh => {
                        let app2 = app.clone();
                        tokio::spawn(async move {
                            App::refresh_task(app2).await;
                        });
                    }
                    Action::FocusNext => {
                        let mut a = app.lock().await;
                        a.focus_next();
                    }
                    Action::FocusPrev => {
                        let mut a = app.lock().await;
                        a.focus_prev();
                    }
                    Action::SelectNext => {
                        let mut a = app.lock().await;
                        a.select_next();
                    }
                    Action::SelectPrev => {
                        let mut a = app.lock().await;
                        a.select_prev();
                    }
                    Action::ToggleList => {
                        let mut a = app.lock().await;
                        a.toggle_list_mode();
                    }
                    Action::NewInvoice => {
                        if let Some(amount_s) = prompt("amount_sompi") {
                            let memo = prompt("memo").unwrap_or_default();
                            if let Ok(amount) = amount_s.parse::<u64>() {
                                let app2 = app.clone();
                                tokio::spawn(async move {
                                    App::create_invoice_task(app2, amount, memo).await;
                                });
                            } else {
                                let mut a = app.lock().await;
                                a.set_status("invalid amount".into(), Color::Red);
                            }
                        }
                    }
                    Action::SimulatePay => {
                        // capture selected id then spawn
                        let id = {
                            let a = app.lock().await;
                            a.selected_invoice_id()
                        };
                        if let Some(invoice_id) = id {
                            let app2 = app.clone();
                            tokio::spawn(async move {
                                App::simulate_payment_task(app2, invoice_id).await;
                            });
                        }
                    }
                    Action::Acknowledge => {
                        let id = {
                            let a = app.lock().await;
                            a.selected_invoice_id()
                        };
                        if let Some(invoice_id) = id {
                            let app2 = app.clone();
                            tokio::spawn(async move {
                                App::ack_task(app2, invoice_id).await;
                            });
                        }
                    }
                    Action::Dispute => {
                        let id = {
                            let a = app.lock().await;
                            a.selected_invoice_id()
                        };
                        if let Some(invoice_id) = id {
                            let app2 = app.clone();
                            tokio::spawn(async move {
                                App::dispute_invoice_task(app2, invoice_id).await;
                            });
                        }
                    }
                    Action::WatcherConfig => {
                        let mut a = app.lock().await;
                        a.open_watcher_config();
                    }
                    Action::ChargeSub => {
                        let id = {
                            let a = app.lock().await;
                            a.selected_subscription_id()
                        };
                        if let Some(sub_id) = id {
                            let app2 = app.clone();
                            tokio::spawn(async move {
                                App::charge_sub_task(app2, sub_id).await;
                            });
                        } else {
                            let mut a = app.lock().await;
                            a.set_status("no subscription selected".into(), Color::Red);
                        }
                    }
                    Action::None => {}
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

fn prompt(msg: &str) -> Option<String> {
    use std::io::{self, Write};
    if disable_raw_mode().is_err() {
        return None;
    }
    let mut stdout = std::io::stdout();
    let _ = write!(stdout, "{msg}: ");
    let _ = stdout.flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        let _ = enable_raw_mode();
        return None;
    }
    let _ = enable_raw_mode();
    Some(input.trim().to_string())
}
