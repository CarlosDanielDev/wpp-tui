//! Ratatui rendering of [`App`] state. Pure view layer: reads state, draws
//! frames, never mutates. A retro-CRT DOS look — double box-drawing borders,
//! phosphor-green/amber on black (mirrors maestro's `retro()` theme), and an
//! F-key status bar along the bottom.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, Focus, Screen};
use crate::qr;

mod chat;

// Retro CRT palette — DOS-era phosphor green / amber on black. Mirrors
// maestro's `retro()` theme. The `chat` submodule reads these via `super::`.
const BG: Color = Color::Rgb(0, 0, 0); // true black (not palette black, which themes tint)
const GREEN: Color = Color::Rgb(0, 255, 65); // phosphor green, primary text
const GREEN_DIM: Color = Color::Rgb(0, 180, 45); // secondary text
const AMBER: Color = Color::Rgb(255, 175, 0); // titles, focus, f-keys
const AMBER_DK: Color = Color::Rgb(255, 140, 0); // active border, selection
const PANEL_BG: Color = Color::Rgb(0, 40, 10); // subtle green-black fill

/// Draw the whole frame for the current screen.
pub fn draw(frame: &mut Frame, app: &App) {
    // Paint the entire terminal solid black first so no themed default
    // background ("fog") shows through any unfilled cell.
    frame.render_widget(
        Block::default().style(Style::default().bg(BG)),
        frame.area(),
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    match app.screen {
        Screen::Login => draw_login(frame, app, chunks[0]),
        Screen::Main => draw_main(frame, app, chunks[0]),
    }
    draw_status_bar(frame, app, chunks[1]);
}

fn dos_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(AMBER_DK).bg(BG))
        .style(Style::default().bg(BG).fg(GREEN))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(AMBER)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        ))
}

/// Like [`dos_block`] but the border brightens (amber) when the region is
/// focused, dims (green) otherwise. (#16 will thread `app.theme` through here.)
fn dos_block_focus(title: &str, focused: bool) -> Block<'_> {
    let border = if focused { AMBER_DK } else { GREEN_DIM };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(border).bg(BG))
        .style(Style::default().bg(BG).fg(GREEN))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(AMBER)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        ))
}

fn draw_login(frame: &mut Frame, app: &App, area: Rect) {
    let block = dos_block("wpp-tui — Pair device");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![Line::from("")];
    match &app.qr {
        Some(code) => {
            lines.push(Line::from(Span::styled(
                "WhatsApp → Settings → Linked Devices → Link a device → scan:",
                Style::default().fg(GREEN_DIM),
            )));
            lines.push(Line::from(""));

            lines.extend(qr::render_qr(code));

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Code expires in ~30s — a new one will appear automatically",
                Style::default().fg(GREEN_DIM),
            )));
        }
        None => {
            // Animated braille spinner while the bridge negotiates the code.
            const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let spin = SPINNER[(app.tick as usize) % SPINNER.len()];
            lines.push(Line::from(vec![
                Span::styled(format!("{spin}  "), Style::default().fg(AMBER)),
                Span::styled("Waiting for QR code…", Style::default().fg(GREEN_DIM)),
                Span::styled(format!("  {spin}"), Style::default().fg(AMBER)),
            ]));
        }
    }
    if app.connected {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Connected!",
            Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        )));
    }

    let para = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .style(Style::default().bg(BG));
    frame.render_widget(para, inner);
}

fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    use crate::app::Kind;

    let focused = app.focus == Focus::Sidebar;
    let rows = app.visible_sidebar();
    let block = dos_block_focus("Chats", focused);

    if rows.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let text = if !app.query.trim().is_empty() {
            "No matches"
        } else if app.connected {
            "No chats yet"
        } else {
            "Connecting…"
        };
        let para = Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(GREEN_DIM),
        )))
        .alignment(Alignment::Center)
        .style(Style::default().bg(BG));
        frame.render_widget(para, inner);
        return;
    }

    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            match row.kind {
                Kind::Contact => {
                    // A contact offered to start a new chat — dim + italic, "(new)".
                    let label = format!("{} (new)", row.name);
                    ListItem::new(Line::from(Span::styled(
                        label,
                        Style::default()
                            .fg(GREEN_DIM)
                            .add_modifier(Modifier::ITALIC),
                    )))
                }
                Kind::Chat => {
                    let label = if row.unread > 0 {
                        format!("{} ({})", row.name, row.unread)
                    } else {
                        row.name.clone()
                    };
                    let style = if row.unread > 0 {
                        Style::default().fg(AMBER).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(GREEN)
                    };
                    ListItem::new(Line::from(Span::styled(label, style)))
                }
            }
        })
        .collect();

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(AMBER_DK)
            .fg(BG)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    state.select(Some(app.selected.min(rows.len().saturating_sub(1))));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(32), Constraint::Min(1)])
        .split(area);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(cols[0]);
    draw_search(frame, app, left[0]);
    draw_sidebar(frame, app, left[1]);
    match app.open_chat.as_ref() {
        None => chat::draw_empty_pane(frame, cols[1]),
        Some(_) => chat::draw_chat_pane(frame, app, cols[1]),
    }
}

fn draw_search(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Search;
    let block = dos_block_focus("Search", focused);
    let cursor = if focused { "_" } else { "" };
    let para = Paragraph::new(Line::from(Span::styled(
        format!("/{}{}", app.query, cursor),
        Style::default().fg(AMBER),
    )))
    .block(block)
    .style(Style::default().bg(BG));
    frame.render_widget(para, area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let keys = match (app.screen, app.focus) {
        (Screen::Login, _) => "Quit[q]",
        (Screen::Main, Focus::Search) => "Type to filter  Enter/Tab:Next  Esc:Chats",
        (Screen::Main, Focus::Sidebar) => "↑↓/jk:Move  Enter:Open  Tab:Search  Quit[q/Esc]",
        (Screen::Main, Focus::Input) => "Type  Enter:Send  Tab:Chats  Esc:Back",
    };
    let bar = Line::from(vec![
        Span::styled(
            format!(" {} ", app.status),
            Style::default()
                .bg(AMBER)
                .fg(BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {keys}"), Style::default().bg(PANEL_BG).fg(GREEN)),
    ]);
    let para = Paragraph::new(bar).style(Style::default().bg(BG));
    frame.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendEvent, Contact, Message};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render(app: &App) -> String {
        // Tall enough to hold a quiet-zoned QR plus the surrounding hint lines.
        let mut terminal = Terminal::new(TestBackend::new(80, 60)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn chat_shows_delivery_marker_for_sent_message() {
        use crate::backend::DeliveryState;
        use crossterm::event::{KeyCode, KeyEvent};
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "Alice".into(),
        }]);
        // Seed a chat so it is in chat_order and Enter opens it (post-#12).
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message::incoming("hey"),
        });
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let mut m = Message::outgoing("m1", "hello");
        m.status = DeliveryState::Read;
        app.messages.entry("a@s".into()).or_default().push(m);
        let out = render(&app);
        assert!(out.contains("hello"));
        assert!(out.contains("✔✔")); // read marker
    }

    #[test]
    fn search_box_shows_query_and_filters_sidebar() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![
            Contact {
                jid: "a@s".into(),
                name: "Alice".into(),
            },
            Contact {
                jid: "b@s".into(),
                name: "Bob".into(),
            },
        ]);
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message::incoming("1"),
        });
        app.apply_event(BackendEvent::Message {
            chat: "b@s".into(),
            msg: Message::incoming("2"),
        });
        app.query = "ali".into();
        let out = render(&app);
        assert!(out.contains("ali")); // query echoed in the search box
        assert!(out.contains("Alice")); // matching chat
        assert!(!out.contains("Bob")); // filtered out
    }

    #[test]
    fn login_screen_shows_qr() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Qr("MOCK-QR-SCAN-ME".into()));
        let out = render(&app);
        assert!(out.contains("Pair device"));
        assert!(out.contains("Linked Devices"));
        assert!(out.contains("Code expires"));
        assert!(!out.contains("MOCK-QR-SCAN-ME"));
        assert!(out.contains('\u{2580}') || out.contains('\u{2584}') || out.contains('\u{2588}'));
    }

    #[test]
    fn sidebar_lists_chats_not_all_contacts() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![
            Contact {
                jid: "a@s".into(),
                name: "Alice".into(),
            },
            Contact {
                jid: "b@s".into(),
                name: "Bob".into(),
            },
        ]);
        // Only Alice has a conversation.
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message::incoming("hi"),
        });
        let out = render(&app);
        assert!(out.contains("Alice"));
        assert!(!out.contains("Bob")); // Bob has no chat → not in the sidebar
        assert!(out.contains("(1)")); // unread badge
    }

    #[test]
    fn empty_sidebar_shows_placeholder_message() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        let out = render(&app);
        assert!(out.contains("Chats"));
        assert!(out.contains("No chats yet"));
    }

    #[test]
    fn sidebar_before_connect_shows_connecting() {
        let app = App {
            screen: Screen::Main,
            ..Default::default()
        };
        let out = render(&app);
        assert!(out.contains("Chats"));
        assert!(out.contains("Connecting"));
    }

    #[test]
    fn main_idle_shows_empty_right_pane() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message::incoming("hi"),
        });
        // No chat opened → right pane is the placeholder, not a chat.
        let out = render(&app);
        assert!(app.open_chat.is_none());
        assert!(out.contains("No chat selected"));
    }

    #[test]
    fn main_open_chat_shows_pane_with_history_and_input() {
        use crossterm::event::{KeyCode, KeyEvent};
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "Alice".into(),
        }]);
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message::incoming("hello there"),
        });
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let out = render(&app);
        assert!(out.contains("Alice")); // pane header
        assert!(out.contains("hello there")); // history
        assert!(out.contains("Message")); // input box title
    }

    #[test]
    fn chat_history_sticks_to_bottom_when_overflowing() {
        use crossterm::event::{KeyCode, KeyEvent};
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "Alice".into(),
        }]);
        for i in 0..60 {
            app.apply_event(BackendEvent::Message {
                chat: "a@s".into(),
                msg: Message::incoming(format!("line-{i}")),
            });
        }
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let out = render(&app);
        assert!(out.contains("line-59"));
        assert!(!out.contains("line-0 "));
    }

    #[test]
    fn contacts_status_bar_shows_jk_navigation() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "Alice".into(),
        }]);
        let out = render(&app);
        assert!(out.contains("jk:Move"));
    }

    #[test]
    fn chat_header_shows_typing_indicator() {
        use crate::backend::Presence;
        use crossterm::event::{KeyCode, KeyEvent};
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "Alice".into(),
        }]);
        // Seed the chat so it lands in `chat_order`, then open it.
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message::incoming("hi"),
        });
        app.on_key(KeyEvent::from(KeyCode::Enter));
        app.apply_event(BackendEvent::Presence {
            chat: "a@s".into(),
            state: Presence::Typing,
        });
        let out = render(&app);
        assert!(out.contains("Alice"));
        assert!(out.contains("typing…"));
    }
}
