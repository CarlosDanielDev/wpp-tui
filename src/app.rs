use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Which top-level screen the app is currently showing. Keypresses are routed
/// to the handler for whichever screen is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// QR-login screen (P2).
    Login,
    /// Contact / recent-chat list (P3).
    Contacts,
    /// One-to-one conversation view (P4).
    Chat,
}

/// App-level state. Owns the active [`Screen`] and dispatches key events to the
/// matching per-screen handler, which may transition to another screen.
pub struct App {
    screen: Screen,
    should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Login,
            should_quit: false,
        }
    }
}

impl App {
    /// Fresh app, starting on the login screen.
    pub fn new() -> Self {
        Self::default()
    }

    /// The screen currently being shown.
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Whether the event loop should exit.
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Route a key event. Global keys (quit) are handled first; everything else
    /// is delegated to the active screen's handler.
    pub fn on_key(&mut self, key: KeyEvent) {
        // Ctrl+C quits from anywhere.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        if key.code == KeyCode::Char('q') {
            self.should_quit = true;
            return;
        }

        match self.screen {
            Screen::Login => self.on_login_key(key),
            Screen::Contacts => self.on_contacts_key(key),
            Screen::Chat => self.on_chat_key(key),
        }
    }

    fn on_login_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Enter {
            self.screen = Screen::Contacts;
        }
    }

    fn on_contacts_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.screen = Screen::Chat,
            KeyCode::Esc => self.screen = Screen::Login,
            _ => {}
        }
    }

    fn on_chat_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.screen = Screen::Contacts;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn new_app_starts_on_login() {
        let app = App::new();
        assert_eq!(app.screen(), Screen::Login);
        assert!(!app.should_quit());
    }

    #[test]
    fn login_enter_goes_to_contacts() {
        let mut app = App::new();
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.screen(), Screen::Contacts);
    }

    #[test]
    fn contacts_enter_goes_to_chat() {
        let mut app = App::new();
        app.on_key(key(KeyCode::Enter)); // -> Contacts
        app.on_key(key(KeyCode::Enter)); // -> Chat
        assert_eq!(app.screen(), Screen::Chat);
    }

    #[test]
    fn contacts_esc_goes_back_to_login() {
        let mut app = App::new();
        app.on_key(key(KeyCode::Enter)); // -> Contacts
        app.on_key(key(KeyCode::Esc)); // -> Login
        assert_eq!(app.screen(), Screen::Login);
    }

    #[test]
    fn chat_esc_goes_back_to_contacts() {
        let mut app = App::new();
        app.on_key(key(KeyCode::Enter)); // -> Contacts
        app.on_key(key(KeyCode::Enter)); // -> Chat
        app.on_key(key(KeyCode::Esc)); // -> Contacts
        assert_eq!(app.screen(), Screen::Contacts);
    }

    #[test]
    fn unrelated_key_does_not_transition() {
        let mut app = App::new();
        app.on_key(key(KeyCode::Char('x')));
        assert_eq!(app.screen(), Screen::Login);
        assert!(!app.should_quit());
    }

    #[test]
    fn q_sets_quit() {
        let mut app = App::new();
        app.on_key(key(KeyCode::Char('q')));
        assert!(app.should_quit());
    }

    #[test]
    fn ctrl_c_sets_quit() {
        let mut app = App::new();
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit());
    }

    #[test]
    fn full_round_trip_login_contacts_chat_and_back() {
        let mut app = App::new();
        assert_eq!(app.screen(), Screen::Login);

        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.screen(), Screen::Contacts);

        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.screen(), Screen::Chat);

        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.screen(), Screen::Contacts);

        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.screen(), Screen::Login);
    }
}
