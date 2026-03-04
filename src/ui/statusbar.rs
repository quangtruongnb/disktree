use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn build_status_bar(
    skipped_count: usize,
    status_message: Option<&str>,
    confirm_trash: bool,
) -> Paragraph<'static> {
    if confirm_trash {
        if let Some(msg) = status_message {
            return Paragraph::new(Line::from(vec![Span::styled(
                msg.to_string(),
                Style::default().fg(Color::Yellow),
            )]));
        }
    }

    let mut spans = vec![
        Span::styled("↑↓ Navigate  ", Style::default().fg(Color::DarkGray)),
        Span::styled("→ Open  ", Style::default().fg(Color::DarkGray)),
        Span::styled("← Back  ", Style::default().fg(Color::DarkGray)),
        Span::styled("r Root  ", Style::default().fg(Color::DarkGray)),
        Span::styled("d Trash  ", Style::default().fg(Color::DarkGray)),
        Span::styled("q Quit", Style::default().fg(Color::DarkGray)),
    ];

    if skipped_count > 0 {
        spans.push(Span::styled(
            format!("  | {} skipped", skipped_count),
            Style::default().fg(Color::Yellow),
        ));
    }

    if let Some(msg) = status_message {
        spans.push(Span::styled(
            format!("  | {}", msg),
            Style::default().fg(Color::Red),
        ));
    }

    Paragraph::new(Line::from(spans))
}
