//! The right-hand chat pane of the two-pane Main screen: the scrollable message
//! history plus the compose input, and the placeholder shown when no chat is
//! open. Split out of `ui/mod.rs` to keep that file under the size cap.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::dos_block_focus;
use crate::app::{App, Focus};
use crate::backend::{DeliveryState, Message};

/// Last `rows` messages — the slice visible when the pane sticks to the bottom.
pub(super) fn visible_tail(messages: &[Message], rows: usize) -> &[Message] {
    let start = messages.len().saturating_sub(rows);
    &messages[start..]
}

/// Max visible lines the compose box grows to before it scrolls internally.
const MAX_INPUT_LINES: usize = 6;

/// Height (including the 2 border rows) the compose box should take for an
/// input of `char_count` chars wrapped at `inner_width`, growing up to
/// `max_lines`. The `+1` reserves room for the trailing cursor glyph.
pub(super) fn input_box_height(char_count: usize, inner_width: usize, max_lines: usize) -> u16 {
    let inner_width = inner_width.max(1);
    let total = (char_count + 1).div_ceil(inner_width).max(1);
    (total.min(max_lines) + 2) as u16
}

/// DOS-style delivery marker for an outgoing message. Heavy check = read.
fn delivery_marker(state: DeliveryState) -> &'static str {
    match state {
        DeliveryState::Sending => "·",
        DeliveryState::Sent => "✓",
        DeliveryState::Delivered => "✓✓",
        DeliveryState::Read => "✔✔",
    }
}

/// Render the open chat: a bottom-stuck history above a compose input box.
pub(super) fn draw_chat_pane(frame: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    // The compose box grows with the draft (wrapped) up to a cap, then scrolls.
    let inner_w = (area.width as usize).saturating_sub(2);
    let char_count = app.input.chars().count();
    let input_h = input_box_height(char_count, inner_w, MAX_INPUT_LINES);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(input_h)])
        .split(area);

    let title = {
        let name = app.open_chat_name().unwrap_or_else(|| "Chat".to_string());
        match app.presence_label() {
            Some(p) => format!("Chat — {name} · {p}"),
            None => format!("Chat — {name}"),
        }
    };
    let history_block = dos_block_focus(&title, t, false);

    // -2 for the history block's top/bottom borders.
    let visible_rows = (rows[0].height as usize).saturating_sub(2);
    let lines: Vec<Line> = visible_tail(app.open_messages(), visible_rows)
        .iter()
        .map(|m| {
            if m.from_me {
                Line::from(vec![
                    Span::styled("→ ", Style::default().fg(t.accent)),
                    Span::styled(m.body.clone(), Style::default().fg(t.primary)),
                    Span::styled(
                        format!(" {}", delivery_marker(m.status)),
                        Style::default().fg(if m.status == DeliveryState::Read {
                            t.accent
                        } else {
                            t.dim
                        }),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled("← ", Style::default().fg(t.dim)),
                    Span::styled(m.body.clone(), Style::default().fg(t.primary)),
                ])
            }
        })
        .collect();
    let history = Paragraph::new(lines)
        .block(history_block)
        .wrap(Wrap { trim: false })
        .style(Style::default().bg(t.bg));
    frame.render_widget(history, rows[0]);

    let input_block = dos_block_focus("Message", t, app.focus == Focus::Input);
    // When the draft is taller than the cap, scroll so the end (where the
    // cursor is) stays visible.
    let total_lines = (char_count + 1).div_ceil(inner_w.max(1)).max(1);
    let scroll = total_lines.saturating_sub(MAX_INPUT_LINES) as u16;
    let input = Paragraph::new(Line::from(vec![
        Span::styled(&app.input, Style::default().fg(t.accent)),
        Span::styled(
            "_",
            Style::default()
                .fg(t.accent)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]))
    .block(input_block)
    .wrap(Wrap { trim: false })
    .scroll((scroll, 0))
    .style(Style::default().bg(t.bg));
    frame.render_widget(input, rows[1]);
}

/// Placeholder shown in the right pane when no chat is open.
pub(super) fn draw_empty_pane(frame: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let block = dos_block_focus("", t, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let para = Paragraph::new(Line::from(Span::styled(
        "No chat selected — pick one on the left",
        Style::default().fg(t.dim),
    )))
    .alignment(Alignment::Center)
    .style(Style::default().bg(t.bg));
    frame.render_widget(para, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_box_grows_with_content_then_caps() {
        // Empty / short drafts stay at one line + 2 borders.
        assert_eq!(input_box_height(0, 40, 6), 3);
        assert_eq!(input_box_height(10, 40, 6), 3);
        // ~3 wrapped lines (81 chars / 40) → 3 + 2 borders.
        assert_eq!(input_box_height(80, 40, 6), 5);
        // Very long draft caps at MAX_INPUT_LINES (6) + 2 borders.
        assert_eq!(input_box_height(1000, 40, 6), 8);
        // Degenerate width is treated as 1 col: never panics, never returns 0
        // (each char its own line → caps at MAX_INPUT_LINES + borders).
        assert_eq!(input_box_height(5, 0, 6), 8);
    }

    #[test]
    fn delivery_markers_are_distinct_per_state() {
        use crate::backend::DeliveryState::*;
        assert_eq!(delivery_marker(Sending), "·");
        assert_eq!(delivery_marker(Sent), "✓");
        assert_eq!(delivery_marker(Delivered), "✓✓");
        assert_eq!(delivery_marker(Read), "✔✔");
    }
}
