mod app;
mod ui;
mod actions;
mod models;
mod logo;

use std::sync::Arc;
use actions::Action;
use app::App;
use tokio::sync::Mutex;
use std::error::Error;
use crossterm::terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, event};
use ratatui::{prelude::*, Terminal};

struct Args {
    merchant_url: String,
    guardian_url: String,
    webhook_secret: String,
    mock_l1: bool,
}

fn parse_args() -> Args {
    let mut merchant_url = String::new();
    let mut guardian_url = String::new();
    let mut webhook_secret = String::new();
    let mut mock_l1 = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--merchant-url" => merchant_url = args.next().unwrap_or_default(),
            "--guardian-url" => guardian_url = args.next().unwrap_or_default(),
            "--webhook-secret" => webhook_secret = args.next().unwrap_or_default(),
            "--mock-l1" => mock_l1 = true,
            _ => {}
        }
    }
    Args { merchant_url, guardian_url, webhook_secret, mock_l1 }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args();
    let app = Arc::new(Mutex::new(App::new(
        args.merchant_url,
        args.guardian_url,
        args.webhook_secret,
        args.mock_l1,
    )));

    // background refresh task
    {
        let app = Arc::clone(&app);
        tokio::spawn(async move {
            loop {
                {
                    let mut app = app.lock().await;
                    app.refresh().await;
                }
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
        eprintln!("{:?}", e);
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
        if event::poll(std::time::Duration::from_millis(100))? {
            if let event::Event::Key(key) = event::read()? {
                let mut app = app.lock().await;
                if let Some(modal) = app.watcher_config.as_mut() {
                    match key.code {
                        event::KeyCode::Esc => app.close_watcher_config(),
                        event::KeyCode::Enter => {
                            app.submit_watcher_config().await;
                        }
                        event::KeyCode::Tab => modal.toggle_mode(),
                        event::KeyCode::Up | event::KeyCode::Down => modal.toggle_field(),
                        event::KeyCode::Backspace => modal.backspace(),
                        event::KeyCode::Char(c) => modal.input_char(c),
                        _ => {}
                    }
                } else {
                    match Action::from_key(key) {
                        Action::Quit => return Ok(()),
                        Action::Refresh => app.refresh().await,
                        Action::FocusNext => app.focus_next(),
                        Action::FocusPrev => app.focus_prev(),
                        Action::SelectNext => app.select_next(),
                        Action::SelectPrev => app.select_prev(),
                        Action::NewInvoice => {
                            if let Some(amount_s) = prompt("amount_sompi") {
                                if let Ok(amount) = amount_s.parse::<u64>() {
                                    let memo = prompt("memo").unwrap_or_default();
                                    app.create_invoice(amount, memo).await;
                                } else {
                                    app.set_status("invalid amount".into(), Color::Red);
                                }
                            }
                        }
                        Action::SimulatePay => {
                            app.simulate_payment().await;
                        }
                        Action::Acknowledge => {
                            app.acknowledge_invoice().await;
                        }
                        Action::WatcherConfig => {
                            app.open_watcher_config();
                        }
                        Action::None => {}
                    }
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
    let _ = write!(stdout, "{}: ", msg);
    let _ = stdout.flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        let _ = enable_raw_mode();
        return None;
    }
    let _ = enable_raw_mode();
    Some(input.trim().to_string())
}
