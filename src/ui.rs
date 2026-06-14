//! Placeholder DOS-style frame. Real screens (login / contacts / chat) land in
//! later P1 issues; this renders the blocky 16-color shell they will fill in.

use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Classic DOS blue field.
const BG: Color = Color::Blue;

/// Render the full-screen placeholder: a double-bordered panel over a blue
/// field with a centered banner and an F-key-style status bar.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Paint the whole screen DOS blue first.
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let panel = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::White).bg(BG))
        .title(Span::styled(
            " wpp-tui ",
            Style::default()
                .fg(Color::Yellow)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(BG));

    let inner = panel.inner(chunks[0]);
    frame.render_widget(panel, chunks[0]);

    let body = Text::from(vec![
        Line::from(""),
        Line::from(Span::styled(
            "DOS-style WhatsApp TUI",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("status: {}", app.status()),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press q or Ctrl+C to quit",
            Style::default().fg(Color::Gray),
        )),
    ]);
    frame.render_widget(
        Paragraph::new(body)
            .alignment(Alignment::Center)
            .style(Style::default().bg(BG)),
        inner,
    );

    let status_bar = Paragraph::new(Line::from(vec![
        Span::styled(" F1 ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::styled(" Help ", Style::default().fg(Color::White).bg(BG)),
        Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::styled(" Quit ", Style::default().fg(Color::White).bg(BG)),
    ]))
    .style(Style::default().bg(BG));
    frame.render_widget(status_bar, chunks[1]);
}
