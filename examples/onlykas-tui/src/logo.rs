use ratatui::{prelude::*, widgets::Paragraph};

const LOGO: [&str; 5] = [
    " ███  ███   █    █   █   █  ██  ███   ████",
    "█   █ █  █  █    █   █   █ ██  █   █ █",
    "█   █ █  █  █     ███    ███   █████  ███",
    "█   █ █  █  █        █   █ ██  █   █     █",
    " ███  █  █  ████  ███    █  ██ █   █ ████",
];

pub fn render_logo() -> Paragraph<'static> {
    // Find the split point using the first line, then convert to a character
    // column index so we can safely style per-char on all lines without
    // splitting at a potentially invalid UTF-8 byte boundary.
    let k_byte_index = LOGO[0].find("█   ██  ███").unwrap_or(0);
    let k_col = LOGO[0].char_indices().position(|(i, _)| i == k_byte_index).unwrap_or(0);

    let white = Style::default().fg(Color::White).bg(Color::Black).add_modifier(Modifier::BOLD);
    // Teal-ish color (RGB) for better contrast than Cyan on some terminals
    let teal = Style::default().fg(Color::Rgb(0, 128, 128)).bg(Color::Black).add_modifier(Modifier::BOLD);

    // Optional ASCII fallback when block drawing looks bad on some terminals
    if std::env::var("ONLYKAS_TUI_ASCII").ok().as_deref() == Some("1") {
        let left = Span::styled("only", white);
        let right = Span::styled("KAS", teal);
        let line = Line::from(vec![left, right]);
        return Paragraph::new(vec![line]).alignment(Alignment::Center);
    }

    let lines: Vec<Line> = LOGO
        .iter()
        .map(|line| {
            let mut col = 0usize;
            let spans = line
                .chars()
                .map(|c| {
                    let style = if col < k_col { white } else { teal };
                    col += 1;
                    Span::styled(c.to_string(), style)
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect();

    Paragraph::new(lines).alignment(Alignment::Center)
}
