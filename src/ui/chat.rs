//! The right-hand chat pane of the two-pane Main screen: the scrollable message
//! history plus the compose input, and the placeholder shown when no chat is
//! open. Split out of `ui/mod.rs` to keep that file under the size cap.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::{dos_block_focus, AMBER, BG, GREEN, GREEN_DIM};
use crate::app::{App, Focus};
use crate::backend::Message;

/// Last `rows` messages — the slice visible when the pane sticks to the bottom.
pub(super) fn visible_tail(messages: &[Message], rows: usize) -> &[Message] {
    let start = messages.len().saturating_sub(rows);
    &messages[start..]
}

/// Render the open chat: a bottom-stuck history above a compose input box.
pub(super) fn draw_chat_pane(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let title = {
        let name = app.open_chat_name().unwrap_or_else(|| "Chat".to_string());
        match app.presence_label() {
            Some(p) => format!("Chat — {name} · {p}"),
            None => format!("Chat — {name}"),
        }
    };
    let history_block = dos_block_focus(&title, false);

    // -2 for the history block's top/bottom borders.
    let visible_rows = (rows[0].height as usize).saturating_sub(2);
    let lines: Vec<Line> = visible_tail(app.open_messages(), visible_rows)
        .iter()
        .map(|m| {
            if m.from_me {
                Line::from(vec![
                    Span::styled("→ ", Style::default().fg(AMBER)),
                    Span::styled(m.body.clone(), Style::default().fg(GREEN)),
                ])
            } else {
                Line::from(vec![
                    Span::styled("← ", Style::default().fg(GREEN_DIM)),
                    Span::styled(m.body.clone(), Style::default().fg(GREEN)),
                ])
            }
        })
        .collect();
    let history = Paragraph::new(lines)
        .block(history_block)
        .wrap(Wrap { trim: false })
        .style(Style::default().bg(BG));
    frame.render_widget(history, rows[0]);

    let input_block = dos_block_focus("Message", app.focus == Focus::Input);
    let input = Paragraph::new(Line::from(vec![
        Span::styled(&app.input, Style::default().fg(AMBER)),
        Span::styled(
            "_",
            Style::default()
                .fg(AMBER)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]))
    .block(input_block)
    .style(Style::default().bg(BG));
    frame.render_widget(input, rows[1]);
}

/// Placeholder shown in the right pane when no chat is open.
pub(super) fn draw_empty_pane(frame: &mut Frame, area: Rect) {
    let block = dos_block_focus("", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let para = Paragraph::new(Line::from(Span::styled(
        "No chat selected — pick one on the left",
        Style::default().fg(GREEN_DIM),
    )))
    .alignment(Alignment::Center)
    .style(Style::default().bg(BG));
    frame.render_widget(para, inner);
}
