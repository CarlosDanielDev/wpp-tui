//! App state and the pure state transitions that drive the TUI.
//!
//! Everything here is side-effect free so it can be unit-tested without a
//! terminal or a live backend: [`App::apply_event`] folds a [`BackendEvent`]
//! into state, and [`App::on_key`] maps a keystroke to an [`Action`] for the
//! event loop to carry out. The terminal I/O lives in `main.rs`.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};

use crate::backend::{BackendEvent, Contact, Message};

/// Which screen is currently shown. Mirrors the P1 routing: login → contacts →
/// chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Waiting for / showing the QR code to pair.
    Login,
    /// The contact / recent-chat list.
    Contacts,
    /// An open one-to-one conversation.
    Chat,
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
}

/// The whole app state. Held by the event loop, mutated only through the
/// methods below.
pub struct App {
    pub screen: Screen,
    /// QR string to render while on the login screen.
    pub qr: Option<String>,
    /// Set once the backend reports a successful pairing.
    pub connected: bool,
    pub contacts: Vec<Contact>,
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
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Login,
            qr: None,
            connected: false,
            contacts: Vec::new(),
            selected: 0,
            messages: HashMap::new(),
            open_chat: None,
            input: String::new(),
            unread: HashMap::new(),
            status: "Waiting for QR code…".to_string(),
            should_quit: false,
        }
    }
}

impl App {
    /// Seed the contact list (fetched once after connecting).
    pub fn set_contacts(&mut self, contacts: Vec<Contact>) {
        self.contacts = contacts;
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
                    self.screen = Screen::Contacts;
                }
                self.status = "Connected".to_string();
            }
            BackendEvent::Message { chat, msg } => {
                let focused =
                    self.screen == Screen::Chat && self.open_chat.as_deref() == Some(chat.as_str());
                self.messages.entry(chat.clone()).or_default().push(msg);
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
            Screen::Contacts => self.on_key_contacts(key),
            Screen::Chat => self.on_key_chat(key),
        }
    }

    fn on_key_login(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            _ => Action::None,
        }
    }

    fn on_key_contacts(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected + 1 < self.contacts.len() {
                    self.selected += 1;
                }
                Action::None
            }
            KeyCode::Enter => {
                if let Some(contact) = self.contacts.get(self.selected) {
                    let jid = contact.jid.clone();
                    self.unread.remove(&jid);
                    self.open_chat = Some(jid);
                    self.screen = Screen::Chat;
                    self.input.clear();
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    fn on_key_chat(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Contacts;
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
                Action::Send { chat, body }
            }
            _ => Action::None,
        }
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
    fn qr_event_populates_login_screen() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Qr("CODE-123".into()));
        assert_eq!(app.screen, Screen::Login);
        assert_eq!(app.qr.as_deref(), Some("CODE-123"));
    }

    #[test]
    fn connected_event_advances_to_contacts() {
        let mut app = App::default();
        assert_eq!(app.screen, Screen::Login);
        app.apply_event(BackendEvent::Connected);
        assert!(app.connected);
        assert_eq!(app.screen, Screen::Contacts);
    }

    #[test]
    fn connected_does_not_yank_user_back_from_chat() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Chat);
        // A re-connect event must not interrupt an open conversation.
        app.apply_event(BackendEvent::Connected);
        assert_eq!(app.screen, Screen::Chat);
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
        app.on_key(key(KeyCode::Enter));
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "hi"),
        });
        assert_eq!(app.open_messages().len(), 1);
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
        let sent = app.open_messages();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].from_me);
        assert_eq!(sent[0].body, "yo");
    }

    #[test]
    fn empty_send_is_noop() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.on_key(key(KeyCode::Enter)), Action::None);
        assert!(app.open_messages().is_empty());
    }

    #[test]
    fn esc_in_chat_returns_to_contacts() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Chat);
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Contacts);
    }

    #[test]
    fn quit_keys_request_quit() {
        let mut app = App::default();
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), Action::Quit);
        app.apply_event(BackendEvent::Connected);
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), Action::Quit);
    }
}
