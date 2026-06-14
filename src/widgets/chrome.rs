//! Reusable DOS-style chrome: a double box-drawing frame with a title bar plus
//! a bottom F-key status bar that wraps a screen's content area.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};

use super::theme;

/// One F-key entry in the bottom status bar, e.g. `F1 Help`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FKey {
    pub n: u8,
    pub label: &'static str,
}

impl FKey {
    pub const fn new(n: u8, label: &'static str) -> Self {
        Self { n, label }
    }
}

/// Screens of the TUI shell. The frame title and status-bar labels are derived
/// from the active screen. Labels are placeholders until the shell wires real
/// actions to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Login,
    Contacts,
    Chat,
}

impl Screen {
    /// Human label shown in the frame's title bar.
    pub fn title(self) -> &'static str {
        match self {
            Screen::Login => "Login",
            Screen::Contacts => "Contacts",
            Screen::Chat => "Chat",
        }
    }

    /// F-key labels for the bottom status bar on this screen.
    pub fn status_keys(self) -> &'static [FKey] {
        match self {
            Screen::Login => LOGIN_KEYS,
            Screen::Contacts => CONTACTS_KEYS,
            Screen::Chat => CHAT_KEYS,
        }
    }
}

const LOGIN_KEYS: &[FKey] = &[
    FKey::new(1, "Help"),
    FKey::new(5, "Refresh"),
    FKey::new(10, "Quit"),
];

const CONTACTS_KEYS: &[FKey] = &[
    FKey::new(1, "Help"),
    FKey::new(3, "Open"),
    FKey::new(5, "Refresh"),
    FKey::new(10, "Quit"),
];

const CHAT_KEYS: &[FKey] = &[
    FKey::new(1, "Help"),
    FKey::new(2, "Send"),
    FKey::new(4, "Back"),
    FKey::new(10, "Quit"),
];

/// Renders the frame (double border + title bar) and the bottom F-key status
/// bar. Stateless; render it every frame. Use [`Chrome::inner`] to get the
/// content rect the screen body should draw into.
#[derive(Debug, Clone)]
pub struct Chrome {
    title: String,
    keys: Vec<FKey>,
}

impl Chrome {
    /// Build chrome with an explicit title and status keys.
    pub fn new(title: impl Into<String>, keys: Vec<FKey>) -> Self {
        Self {
            title: title.into(),
            keys,
        }
    }

    /// Build chrome from a screen using its default title and status keys.
    pub fn for_screen(screen: Screen) -> Self {
        Self {
            title: format!(" wpp-tui — {} ", screen.title()),
            keys: screen.status_keys().to_vec(),
        }
    }

    /// Content rect inside the frame (border and status bar removed) for the
    /// full `area` the chrome occupies.
    pub fn inner(area: Rect) -> Rect {
        let [main, _status] = Self::split(area);
        Block::default().borders(Borders::ALL).inner(main)
    }

    /// Split `area` into the framed-body rect and the 1-row status-bar rect.
    fn split(area: Rect) -> [Rect; 2] {
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area)
    }

    fn render_status(&self, area: Rect, buf: &mut Buffer) {
        let key_style = Style::default()
            .fg(theme::STATUS_KEY_FG)
            .bg(theme::STATUS_KEY_BG);
        let label_style = Style::default().fg(theme::STATUS_FG).bg(theme::STATUS_BG);

        let mut spans: Vec<Span> = Vec::new();
        for key in &self.keys {
            spans.push(Span::styled(format!("F{}", key.n), key_style));
            spans.push(Span::styled(format!("{} ", key.label), label_style));
        }

        Paragraph::new(Line::from(spans))
            .style(label_style)
            .render(area, buf);
    }
}

impl Widget for &Chrome {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [main, status] = Chrome::split(area);

        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(theme::BORDER).bg(theme::BG))
            .title(self.title.as_str())
            .title_style(Style::default().fg(theme::TITLE_FG).bg(theme::TITLE_BG))
            .style(Style::default().fg(theme::FG).bg(theme::BG))
            .render(main, buf);

        self.render_status(status, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    fn row_string(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect()
    }

    #[test]
    fn inner_excludes_border_and_status_bar() {
        let area = Rect::new(0, 0, 40, 10);
        let inner = Chrome::inner(area);

        assert_eq!(inner.x, 1, "left border consumed one column");
        assert_eq!(inner.y, 1, "top border consumed one row");
        // -2 columns for left/right border.
        assert_eq!(inner.width, 38);
        // -2 rows for top/bottom border, -1 for the status bar.
        assert_eq!(inner.height, 7);
    }

    #[test]
    fn frame_has_double_border_corners() {
        let area = Rect::new(0, 0, 20, 6);
        let mut buf = Buffer::empty(area);
        (&Chrome::for_screen(Screen::Chat)).render(area, &mut buf);

        // Body rect is rows 0..5 (last row is the status bar).
        assert_eq!(buf[(0, 0)].symbol(), "╔");
        assert_eq!(buf[(19, 0)].symbol(), "╗");
        assert_eq!(buf[(0, 4)].symbol(), "╚");
        assert_eq!(buf[(19, 4)].symbol(), "╝");
    }

    #[test]
    fn title_bar_shows_screen_name() {
        let area = Rect::new(0, 0, 40, 6);
        let mut buf = Buffer::empty(area);
        (&Chrome::for_screen(Screen::Contacts)).render(area, &mut buf);

        let top = row_string(&buf, 0);
        assert!(top.contains("wpp-tui"), "got: {top:?}");
        assert!(top.contains("Contacts"), "got: {top:?}");
    }

    #[test]
    fn status_bar_lists_screen_fkeys() {
        let area = Rect::new(0, 0, 60, 8);
        let mut buf = Buffer::empty(area);
        (&Chrome::for_screen(Screen::Login)).render(area, &mut buf);

        let status = row_string(&buf, area.height - 1);
        assert!(status.contains("F1"), "got: {status:?}");
        assert!(status.contains("Help"), "got: {status:?}");
        assert!(status.contains("F10"), "got: {status:?}");
        assert!(status.contains("Quit"), "got: {status:?}");
    }

    #[test]
    fn screens_have_distinct_status_keys() {
        assert_ne!(Screen::Login.status_keys(), Screen::Chat.status_keys());
        assert!(Screen::Contacts.status_keys().iter().any(|k| k.n == 3));
        assert!(Screen::Chat.status_keys().iter().any(|k| k.label == "Send"));
    }

    #[test]
    fn every_screen_offers_quit() {
        for screen in [Screen::Login, Screen::Contacts, Screen::Chat] {
            assert!(
                screen.status_keys().iter().any(|k| k.label == "Quit"),
                "{screen:?} missing Quit"
            );
        }
    }
}
