use ratatui::{prelude::*, widgets::*};
use crate::app::{App, Focus};
use crate::models::invoice_to_string;
use crate::logo;

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
    render_invoices(f, app, chunks[2]);
    render_watcher(f, app, chunks[3]);
    render_guardian(f, app, chunks[4]);
    render_webhooks(f, app, chunks[5]);

    let status = Paragraph::new(format!("focus: {:?}", app.focus));
    f.render_widget(status, chunks[6]);
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
        Line::raw("arrows: navigate"),
    ];
    f.render_widget(Paragraph::new(items).block(block), area);
}

fn render_invoices<B: Backend>(f: &mut Frame<B>, app: &App, area: Rect) {
    let block = panel_block("Invoices", app.focus == Focus::Invoices);
    let items: Vec<ListItem> = app
        .invoices
        .iter()
        .map(|i| ListItem::new(invoice_to_string(i)))
        .collect();
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
    f.render_widget(Paragraph::new(format!("{}", app.guardian)).block(block), area);
}

fn render_webhooks<B: Backend>(f: &mut Frame<B>, app: &App, area: Rect) {
    let block = panel_block("Webhooks", app.focus == Focus::Webhooks);
    f.render_widget(Paragraph::new("None").block(block), area);
}
