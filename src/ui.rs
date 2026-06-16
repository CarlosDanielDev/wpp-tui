//! Ratatui rendering of [`App`] state. Pure view layer: reads state, draws
//! frames, never mutates. A retro-CRT DOS look — double box-drawing borders,
//! phosphor-green/amber on black (mirrors maestro's `retro()` theme), and an
//! F-key status bar along the bottom.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, Screen};
use crate::qr;

// Retro CRT palette — DOS-era phosphor green / amber on black. Mirrors
// maestro's `retro()` theme.
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

fn draw_contacts(frame: &mut Frame, app: &App, area: Rect) {
    let block = dos_block("Contacts");

    if app.contacts.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let text = if app.connected {
            "No contacts yet — syncing… press r to refresh"
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

    let items: Vec<ListItem> = app
        .contacts
        .iter()
        .map(|c| {
            let unread = app.unread.get(&c.jid).copied().unwrap_or(0);
            let label = if unread > 0 {
                format!("{} ({unread})", c.name)
            } else {
                c.name.clone()
            };
            let style = if unread > 0 {
                Style::default().fg(AMBER).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(GREEN)
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(AMBER_DK)
            .fg(BG)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_chat(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let title = app
        .open_chat_name()
        .map(|n| format!("Chat — {n}"))
        .unwrap_or_else(|| "Chat".to_string());
    let history_block = dos_block(&title);

    let lines: Vec<Line> = app
        .open_messages()
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

    let input_block = dos_block("Message");
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

// Temporary shim: Tasks 3–4 build the real two-pane `draw_main`.
fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    draw_contacts(frame, app, area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let keys = match (app.screen, app.focus) {
        (Screen::Login, _) => "F:Quit[q]",
        (Screen::Main, Focus::Sidebar) => "↑↓/jk:Move  Enter:Open  r:Refresh  Quit[q]",
        (Screen::Main, Focus::Input) => "Type:Compose  Enter:Send  Esc:Back",
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
    fn contacts_screen_lists_names_and_unread() {
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
            msg: Message {
                from_me: false,
                body: "hi".into(),
            },
        });
        let out = render(&app);
        assert!(out.contains("Alice"));
        assert!(out.contains("Bob"));
        // Unread badge.
        assert!(out.contains("(1)"));
    }

    #[test]
    // Superseded by Task 4's `main_open_chat_shows_pane_with_history_and_input`;
    // the `draw_main` shim cannot render the chat pane yet. Removed in Task 4.
    #[ignore]
    fn chat_screen_shows_messages() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "Alice".into(),
        }]);
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message {
                from_me: false,
                body: "hello there".into(),
            },
        });
        // Open Alice's chat.
        use crossterm::event::{KeyCode, KeyEvent};
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let out = render(&app);
        assert!(out.contains("hello there"));
        assert!(out.contains("Alice"));
    }

    #[test]
    fn empty_contacts_shows_placeholder_message() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        let out = render(&app);
        assert!(out.contains("Contacts"));
        assert!(out.contains("No contacts yet"));
    }

    #[test]
    fn contacts_screen_before_connect_shows_connecting() {
        let app = App {
            screen: Screen::Main,
            ..Default::default()
        };
        let out = render(&app);
        assert!(out.contains("Contacts"));
        assert!(out.contains("Connecting"));
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
}
