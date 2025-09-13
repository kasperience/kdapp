use ratatui::{prelude::*, widgets::Paragraph};

pub fn onlykas_logo() -> Paragraph<'static> {
    const ONLY: [&str; 5] = [
        " ███  ███   █    █   █",
        "█   █ █  █  █    █   █",
        "█   █ █  █  █     ███",
        "█   █ █  █  █        █",
        " ███  █  █  ████  ███",
    ];

    const KAS: [&str; 5] = [
        "█  ██  ███   ████",
        "█ ██  █   █ █",
        "███   █████  ███",
        "█ ██  █   █     █",
        "█  ██ █   █ ████",
    ];

    let left_width = ONLY.iter().map(|l| l.len()).max().unwrap_or(0);
    let lines: Vec<Line> = ONLY
        .iter()
        .zip(KAS.iter())
        .map(|(o, k)| {
            let padded = format!("{:<width$}", o, width = left_width);
            Line::from(vec![
                Span::styled(
                    padded,
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    *k,
                    Style::default()
                        .fg(Color::Cyan)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        })
        .collect();

    Paragraph::new(lines).alignment(Alignment::Center)
}
