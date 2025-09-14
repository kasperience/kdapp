use ratatui::{prelude::*, widgets::Paragraph};

const LOGO: [&str; 5] = [
    " ███  ███   █    █   ██  ██  ███   ████",
    "█   █ █  █  █    █   ██ ██  █   █ █",
    "█   █ █  █  █     ███ ███   █████  ███",
    "█   █ █  █  █        ██ ██  █   █     █",
    " ███  █  █  ████  ███ █  ██ █   █ ████",
];

pub fn render_logo() -> Paragraph<'static> {
    let k_index = LOGO[0].find("█  ██  ███").unwrap_or(0);
    let white = Style::default()
        .fg(Color::White)
        .bg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let teal = Style::default()
        .fg(Color::Cyan)
        .bg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let lines: Vec<Line> = LOGO
        .iter()
        .map(|line| {
            let (left, right) = line.split_at(k_index);
            let spans = left
                .chars()
                .map(|c| Span::styled(c.to_string(), white))
                .chain(right.chars().map(|c| Span::styled(c.to_string(), teal)))
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect();

    Paragraph::new(lines).alignment(Alignment::Center)
}

