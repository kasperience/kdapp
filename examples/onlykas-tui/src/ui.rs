use crate::{
    app::{App, Focus, ListMode, WatcherConfigModal, WatcherField},
    logo,
    models::{invoice_to_string, subscription_to_string},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &App) {
    let size = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(3),
            Constraint::Min(3),
            Constraint::Min(3),
            Constraint::Min(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(size);

    let header = Paragraph::new(logo::onlykas_logo()).alignment(Alignment::Center);
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
}

fn panel_block(title: &str, focused: bool) -> Block {
    let mut block = Block::default().title(title).borders(Borders::ALL);
    if focused {
        block = block.border_style(Style::default().fg(Color::Yellow));
    }
    block
}

fn render_actions<B: Backend>(f: &mut Frame<B>, app: &App, area: Rect) {
    let block = panel_block("Actions", app.focus == Focus::Actions);
    let items = vec![
        Line::raw("q: quit"),
        Line::raw("r: refresh"),
        Line::raw("tab: toggle list"),
        Line::raw("n: new invoice"),
        Line::raw("p: simulate pay"),
        Line::raw("a: acknowledge"),
        Line::raw("d: dispute"),
        Line::raw("s: charge sub"),
        Line::raw("w: watcher config"),
        Line::raw("arrows: navigate"),
    ];
    f.render_widget(Paragraph::new(items).block(block), area);
}

fn render_items<B: Backend>(f: &mut Frame<B>, app: &App, area: Rect) {
    let title = match app.list_mode {
        ListMode::Invoices => "Invoices",
        ListMode::Subscriptions => "Subscriptions",
    };
    let block = panel_block(title, app.focus == Focus::Invoices);
    let items: Vec<ListItem> = match app.list_mode {
        ListMode::Invoices => app
            .invoices
            .iter()
            .map(|i| ListItem::new(invoice_to_string(i)))
            .collect(),
        ListMode::Subscriptions => app
            .subscriptions
            .iter()
            .map(|s| ListItem::new(subscription_to_string(s)))
            .collect(),
    };
    let mut state = ListState::default();
    state.select(Some(app.selection));
    let list = List::new(items).block(block).highlight_style(Style::default().bg(Color::Blue));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_watcher<B: Backend>(f: &mut Frame<B>, app: &App, area: Rect) {
    let block = panel_block("Watcher", app.focus == Focus::Watcher);
    f.render_widget(Paragraph::new(format!("{}", app.watcher)).block(block), area);
}

fn render_guardian<B: Backend>(f: &mut Frame<B>, app: &App, area: Rect) {
    let block = panel_block("Guardian", app.focus == Focus::Guardian);
    let text = if let Some(obj) = app.guardian.as_object() {
        let disputes = obj
            .get("disputes_open")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let refunds = obj
            .get("refunds_signed")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        format!("disputes_open: {}\nrefunds_signed: {}", disputes, refunds)
    } else {
        format!("{}", app.guardian)
    };
    f.render_widget(Paragraph::new(text).block(block), area);
}

fn render_webhooks<B: Backend>(f: &mut Frame<B>, app: &App, area: Rect) {
    let block = panel_block("Webhooks", app.focus == Focus::Webhooks);
    if app.webhooks.is_empty() {
        f.render_widget(Paragraph::new("None").block(block), area);
    } else {
        let items: Vec<ListItem> = app
            .webhooks
            .iter()
            .map(|w| {
                ListItem::new(format!("{} id={} ts={} {}", w.event, w.id, w.ts, w.details))
            })
            .collect();
        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }
}

fn render_watcher_modal<B: Backend>(f: &mut Frame<B>, modal: &WatcherConfigModal) {
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
