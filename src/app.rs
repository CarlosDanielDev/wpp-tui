//! App state and the pure state transitions that drive the TUI.
//!
//! Everything here is side-effect free so it can be unit-tested without a
//! terminal or a live backend: [`App::apply_event`] folds a [`BackendEvent`]
//! into state, and [`App::on_key`] maps a keystroke to an [`Action`] for the
//! event loop to carry out. The terminal I/O lives in `main.rs`.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};

use crate::backend::{BackendEvent, Contact, Message};

/// Top-level screen. Login is the QR pairing screen; Main is the persistent
/// two-pane layout shown after connecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Login,
    Main,
}

/// Which region of the Main screen has keyboard focus. (#29 adds `Search`.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Input,
}

/// A side effect the event loop should perform after a keystroke. Returning an
/// action (instead of doing I/O here) keeps [`App::on_key`] pure and testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Nothing to do.
    None,
    /// Tear down the TUI and exit.
    Quit,
    /// Send `body` to the chat identified by `chat`.
    Send { chat: String, body: String },
    /// A chat was opened — load its persisted history.
    OpenChat { chat: String },
    /// Re-fetch the contact list from the backend.
    Refresh,
}

/// The whole app state. Held by the event loop, mutated only through the
/// methods below.
pub struct App {
    pub screen: Screen,
    /// Which region of the Main screen has keyboard focus.
    pub focus: Focus,
    /// QR string to render while on the login screen.
    pub qr: Option<String>,
    /// Set once the backend reports a successful pairing.
    pub connected: bool,
    pub contacts: Vec<Contact>,
    /// JIDs that have messages, most-recent-activity first. Drives the sidebar.
    pub chat_order: Vec<String>,
    /// Cursor into `contacts` on the contacts screen.
    pub selected: usize,
    /// Per-chat message history, keyed by chat JID.
    pub messages: HashMap<String, Vec<Message>>,
    /// JID of the chat currently open on the chat screen.
    pub open_chat: Option<String>,
    /// Draft text being composed on the chat screen.
    pub input: String,
    /// Chats with messages received while not focused, keyed by JID.
    pub unread: HashMap<String, usize>,
    /// Transient status line message.
    pub status: String,
    /// Set when the user asks to quit.
    pub should_quit: bool,
    /// Monotonic animation counter, bumped on each render tick. Drives spinners.
    pub tick: u64,
    /// Chats whose persisted history has already been loaded, to avoid
    /// re-loading (and duplicating) on every reopen.
    pub history_loaded: std::collections::HashSet<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Login,
            focus: Focus::Sidebar,
            qr: None,
            connected: false,
            contacts: Vec::new(),
            chat_order: Vec::new(),
            selected: 0,
            messages: HashMap::new(),
            open_chat: None,
            input: String::new(),
            unread: HashMap::new(),
            status: "Waiting for QR code…".to_string(),
            should_quit: false,
            tick: 0,
            history_loaded: std::collections::HashSet::new(),
        }
    }
}

impl App {
    /// Advance the animation counter one render tick.
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Seed the contact list (fetched once after connecting).
    pub fn set_contacts(&mut self, contacts: Vec<Contact>) {
        self.contacts = contacts;
    }

    /// Move `chat` to the front of the sidebar order (insert if new).
    fn front_chat(&mut self, chat: &str) {
        if let Some(pos) = self.chat_order.iter().position(|c| c == chat) {
            self.chat_order.remove(pos);
        }
        self.chat_order.insert(0, chat.to_string());
    }

    /// Fold a backend event into state. Pure: no I/O, no redraw.
    pub fn apply_event(&mut self, event: BackendEvent) {
        match event {
            BackendEvent::Qr(code) => {
                self.qr = Some(code);
                self.screen = Screen::Login;
                self.status = "Scan the QR code to pair".to_string();
            }
            BackendEvent::Connected => {
                self.connected = true;
                // Advance off the login screen on first connect only.
                if self.screen == Screen::Login {
                    self.screen = Screen::Main;
                    self.focus = Focus::Sidebar;
                }
                self.status = "Connected".to_string();
            }
            BackendEvent::Message { chat, msg } => {
                let focused =
                    self.focus == Focus::Input && self.open_chat.as_deref() == Some(chat.as_str());
                self.messages.entry(chat.clone()).or_default().push(msg);
                self.front_chat(&chat);
                if !focused {
                    *self.unread.entry(chat).or_insert(0) += 1;
                }
            }
        }
    }

    /// Map a keystroke to an [`Action`], mutating navigation state as needed.
    pub fn on_key(&mut self, key: KeyEvent) -> Action {
        match self.screen {
            Screen::Login => self.on_key_login(key),
            Screen::Main => match self.focus {
                Focus::Sidebar => self.on_key_sidebar(key),
                Focus::Input => self.on_key_input(key),
            },
        }
    }

    fn on_key_login(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            _ => Action::None,
        }
    }

    fn on_key_sidebar(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char('r') | KeyCode::F(5) => Action::Refresh,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected + 1 < self.chat_order.len() {
                    self.selected += 1;
                }
                Action::None
            }
            KeyCode::Enter => {
                if let Some(jid) = self.chat_order.get(self.selected).cloned() {
                    self.unread.remove(&jid);
                    self.open_chat = Some(jid.clone());
                    self.focus = Focus::Input;
                    self.input.clear();
                    return Action::OpenChat { chat: jid };
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    fn on_key_input(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Sidebar;
                self.input.clear();
                Action::None
            }
            KeyCode::Backspace => {
                self.input.pop();
                Action::None
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                Action::None
            }
            KeyCode::Enter => {
                let body = self.input.trim().to_string();
                if body.is_empty() {
                    return Action::None;
                }
                self.input.clear();
                let Some(chat) = self.open_chat.clone() else {
                    return Action::None;
                };
                // Echo locally so the sent line appears immediately; the event
                // loop performs the actual backend send.
                self.messages
                    .entry(chat.clone())
                    .or_default()
                    .push(Message {
                        from_me: true,
                        body: body.clone(),
                    });
                self.front_chat(&chat);
                Action::Send { chat, body }
            }
            _ => Action::None,
        }
    }

    /// Fold persisted history into the cache for a chat the first time it is
    /// opened. Skips chats that already have any messages (live or loaded) so
    /// it never duplicates or clobbers live traffic.
    pub fn load_history(&mut self, chat: String, history: Vec<Message>) {
        if self.history_loaded.contains(&chat) || self.messages.contains_key(&chat) {
            self.history_loaded.insert(chat);
            return;
        }
        self.history_loaded.insert(chat.clone());
        self.messages.insert(chat, history);
    }

    /// Messages for the currently open chat (empty slice if none).
    pub fn open_messages(&self) -> &[Message] {
        self.open_chat
            .as_ref()
            .and_then(|jid| self.messages.get(jid))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Display name for the open chat, falling back to the raw JID.
    pub fn open_chat_name(&self) -> Option<String> {
        let jid = self.open_chat.as_ref()?;
        let name = self
            .contacts
            .iter()
            .find(|c| &c.jid == jid)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| jid.clone());
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    fn msg(from_me: bool, body: &str) -> Message {
        Message {
            from_me,
            body: body.to_string(),
        }
    }

    #[test]
    fn opening_a_chat_returns_openchat_action() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "hi"),
        });
        let action = app.on_key(key(KeyCode::Enter));
        assert_eq!(action, Action::OpenChat { chat: "a@s".into() });
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.focus, Focus::Input);
    }

    #[test]
    fn load_history_fills_empty_chat_once() {
        let mut app = App::default();
        app.load_history("a@s".into(), vec![msg(false, "old1"), msg(true, "old2")]);
        assert_eq!(app.messages.get("a@s").map(Vec::len), Some(2));
        // Second load (e.g. reopening) must not duplicate.
        app.load_history("a@s".into(), vec![msg(false, "old1"), msg(true, "old2")]);
        assert_eq!(app.messages.get("a@s").map(Vec::len), Some(2));
    }

    #[test]
    fn load_history_does_not_clobber_live_messages() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "live"),
        });
        // History load after a live message has arrived is skipped for that chat.
        app.load_history("a@s".into(), vec![msg(false, "old")]);
        assert_eq!(app.messages.get("a@s").map(Vec::len), Some(1));
        assert_eq!(app.messages["a@s"][0].body, "live");
    }

    #[test]
    fn qr_event_populates_login_screen() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Qr("CODE-123".into()));
        assert_eq!(app.screen, Screen::Login);
        assert_eq!(app.qr.as_deref(), Some("CODE-123"));
    }

    #[test]
    fn connected_event_enters_main_focused_on_sidebar() {
        let mut app = App::default();
        assert_eq!(app.screen, Screen::Login);
        app.apply_event(BackendEvent::Connected);
        assert!(app.connected);
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn connected_does_not_yank_user_back_from_chat() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "hi"),
        });
        app.on_key(key(KeyCode::Enter));
        assert!(app.open_chat.is_some());
        // A re-connect event must not interrupt an open conversation: the
        // `Connected` arm only re-screens from `Login`.
        app.apply_event(BackendEvent::Connected);
        assert!(app.open_chat.is_some());
    }

    #[test]
    fn incoming_message_is_stored_and_marked_unread() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "hi"),
        });
        assert_eq!(app.messages.get("a@s").map(Vec::len), Some(1));
        assert_eq!(app.unread.get("a@s"), Some(&1));
    }

    #[test]
    fn message_to_open_chat_is_not_unread() {
        let mut app = App::default();
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.apply_event(BackendEvent::Connected);
        // Seed the chat so it appears in `chat_order`, then open it (clears unread).
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "seed"),
        });
        app.on_key(key(KeyCode::Enter));
        // A further message to the focused chat must not mark it unread.
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "hi"),
        });
        assert_eq!(app.open_messages().len(), 2);
        assert_eq!(app.unread.get("a@s"), None);
    }

    #[test]
    fn contacts_navigation_clamps() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![
            Contact {
                jid: "a@s".into(),
                name: "A".into(),
            },
            Contact {
                jid: "b@s".into(),
                name: "B".into(),
            },
        ]);
        // Sidebar navigates `chat_order`; seed two chats via incoming messages.
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "1"),
        });
        app.apply_event(BackendEvent::Message {
            chat: "b@s".into(),
            msg: msg(false, "2"),
        });
        app.on_key(key(KeyCode::Up)); // already at top
        assert_eq!(app.selected, 0);
        app.on_key(key(KeyCode::Down));
        assert_eq!(app.selected, 1);
        app.on_key(key(KeyCode::Down)); // clamp at bottom
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn opening_chat_clears_unread() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "hi"),
        });
        assert_eq!(app.unread.get("a@s"), Some(&1));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.unread.get("a@s"), None);
    }

    #[test]
    fn typing_and_sending_echoes_and_returns_action() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "hi"),
        });
        app.on_key(key(KeyCode::Enter)); // open chat
        for c in "yo".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.input, "yo");
        let action = app.on_key(key(KeyCode::Enter));
        assert_eq!(
            action,
            Action::Send {
                chat: "a@s".into(),
                body: "yo".into()
            }
        );
        assert!(app.input.is_empty());
        // Seeded incoming "hi" plus the echoed outgoing "yo".
        let sent = app.open_messages();
        assert_eq!(sent.len(), 2);
        let last = sent.last().unwrap();
        assert!(last.from_me);
        assert_eq!(last.body, "yo");
    }

    #[test]
    fn empty_send_is_noop() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "hi"),
        });
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.on_key(key(KeyCode::Enter)), Action::None);
        // Only the seeded incoming message exists; the empty send added nothing.
        assert_eq!(app.open_messages().len(), 1);
    }

    #[test]
    fn esc_in_input_returns_to_sidebar() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "hi"),
        });
        app.on_key(key(KeyCode::Enter)); // open a@s, focus → Input
        assert_eq!(app.focus, Focus::Input);
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn incoming_message_fronts_chat_order() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "1"),
        });
        app.apply_event(BackendEvent::Message {
            chat: "b@s".into(),
            msg: msg(false, "2"),
        });
        assert_eq!(app.chat_order, vec!["b@s".to_string(), "a@s".to_string()]);
        // New activity on an existing chat moves it to the front, no duplicate.
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "3"),
        });
        assert_eq!(app.chat_order, vec!["a@s".to_string(), "b@s".to_string()]);
    }

    #[test]
    fn sending_fronts_chat_order() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.apply_event(BackendEvent::Message {
            chat: "z@s".into(),
            msg: msg(false, "x"),
        });
        app.open_chat = Some("a@s".into());
        app.focus = Focus::Input;
        for c in "hi".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter)); // send
        assert_eq!(app.chat_order.first().map(String::as_str), Some("a@s"));
    }

    #[test]
    fn quit_keys_request_quit() {
        let mut app = App::default();
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), Action::Quit);
        app.apply_event(BackendEvent::Connected);
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), Action::Quit);
    }
}
