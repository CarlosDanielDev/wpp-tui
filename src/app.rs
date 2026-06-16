//! App state and the pure state transitions that drive the TUI.
//!
//! Everything here is side-effect free so it can be unit-tested without a
//! terminal or a live backend: [`App::apply_event`] folds a [`BackendEvent`]
//! into state, and [`App::on_key`] maps a keystroke to an [`Action`] for the
//! event loop to carry out. The terminal I/O lives in `main.rs`.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};

use crate::backend::{BackendEvent, Contact, DeliveryState, Message, Presence};
use crate::fuzzy::fuzzy_score;

/// Whether a sidebar row is an existing chat or a contact offered to start one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Chat,
    Contact,
}

/// One rendered sidebar row.
#[derive(Debug, Clone)]
pub struct Row {
    pub jid: String,
    pub name: String,
    pub unread: usize,
    pub kind: Kind,
}

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
    Search,
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
    /// Send `body` to the chat identified by `chat`, stamped with local `id`.
    Send {
        id: String,
        chat: String,
        body: String,
    },
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
    /// Live fuzzy-search query driving the sidebar filter (#29).
    pub query: String,
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
    /// Latest presence per chat JID.
    pub presence: std::collections::HashMap<String, Presence>,
    /// Monotonic counter for stamping outgoing messages with a unique local id.
    pub msg_seq: u64,
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
            query: String::new(),
            messages: HashMap::new(),
            open_chat: None,
            input: String::new(),
            unread: HashMap::new(),
            status: "Waiting for QR code…".to_string(),
            should_quit: false,
            tick: 0,
            history_loaded: std::collections::HashSet::new(),
            presence: std::collections::HashMap::new(),
            msg_seq: 0,
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

    fn display_name(&self, jid: &str) -> String {
        self.contacts
            .iter()
            .find(|c| c.jid == jid)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| jid.to_string())
    }

    /// The sidebar rows for the current query: chats first; if none match, fall
    /// back to fuzzy-matched contacts.
    pub fn visible_sidebar(&self) -> Vec<Row> {
        let q = self.query.trim();
        // Chat rows (fuzzy-filtered when there's a query), preserving chat_order
        // on empty query and sorting by score otherwise.
        let mut chats: Vec<(i32, Row)> = self
            .chat_order
            .iter()
            .filter_map(|jid| {
                let name = self.display_name(jid);
                let score = if q.is_empty() {
                    Some(0)
                } else {
                    fuzzy_score(q, &name).or_else(|| fuzzy_score(q, jid))
                }?;
                Some((
                    score,
                    Row {
                        jid: jid.clone(),
                        name,
                        unread: self.unread.get(jid).copied().unwrap_or(0),
                        kind: Kind::Chat,
                    },
                ))
            })
            .collect();
        if !q.is_empty() {
            // Stable: higher score first, original order on ties.
            chats.sort_by(|a, b| b.0.cmp(&a.0));
        }
        if !chats.is_empty() || q.is_empty() {
            return chats.into_iter().map(|(_, r)| r).collect();
        }
        // Zero chat matches and a non-empty query → contact fallback.
        let mut contacts: Vec<(i32, Row)> = self
            .contacts
            .iter()
            .filter_map(|c| {
                let score = fuzzy_score(q, &c.name).or_else(|| fuzzy_score(q, &c.jid))?;
                Some((
                    score,
                    Row {
                        jid: c.jid.clone(),
                        name: c.name.clone(),
                        unread: 0,
                        kind: Kind::Contact,
                    },
                ))
            })
            .collect();
        contacts.sort_by(|a, b| b.0.cmp(&a.0));
        contacts.into_iter().map(|(_, r)| r).collect()
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
            BackendEvent::Presence { chat, state } => {
                self.presence.insert(chat, state);
            }
            BackendEvent::Receipt { chat, ids, state } => {
                if let Some(msgs) = self.messages.get_mut(&chat) {
                    for m in msgs.iter_mut() {
                        if ids.iter().any(|id| id == &m.id) && state.rank() > m.status.rank() {
                            m.status = state;
                        }
                    }
                }
            }
        }
    }

    /// Map a keystroke to an [`Action`], mutating navigation state as needed.
    pub fn on_key(&mut self, key: KeyEvent) -> Action {
        match self.screen {
            Screen::Login => self.on_key_login(key),
            Screen::Main => match self.focus {
                Focus::Search => self.on_key_search(key),
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

    fn on_key_search(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Tab => {
                self.focus = Focus::Input;
                Action::None
            }
            KeyCode::Esc => {
                self.focus = Focus::Sidebar;
                Action::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
                Action::None
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.selected = 0;
                Action::None
            }
            KeyCode::Enter => {
                self.focus = Focus::Sidebar;
                Action::None
            } // hand off to sidebar to open
            _ => Action::None,
        }
    }

    fn on_key_sidebar(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Tab => {
                self.focus = Focus::Search;
                Action::None
            }
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
            KeyCode::Tab => {
                self.focus = Focus::Sidebar;
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
                self.msg_seq += 1;
                let id = format!("local-{}", self.msg_seq);
                let mut m = Message::outgoing(id.clone(), body.clone());
                // Echoed immediately as Sent; receipts promote it from here.
                m.status = DeliveryState::Sent;
                self.messages.entry(chat.clone()).or_default().push(m);
                self.front_chat(&chat);
                Action::Send { id, chat, body }
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

    /// A short presence string for the open chat, if known.
    pub fn presence_label(&self) -> Option<String> {
        let jid = self.open_chat.as_ref()?;
        match self.presence.get(jid)? {
            Presence::Typing => Some("typing…".to_string()),
            Presence::Online => Some("online".to_string()),
            Presence::Offline { last_seen: Some(t) } => Some(format!("last seen {t}")),
            Presence::Offline { last_seen: None } => Some("offline".to_string()),
        }
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
            id: String::new(),
            from_me,
            body: body.to_string(),
            status: DeliveryState::Sent,
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
        assert!(matches!(
            action,
            Action::Send {
                id: _,
                chat,
                body,
            } if chat == "a@s" && body == "yo"
        ));
        assert!(app.input.is_empty());
        // Seeded incoming "hi" plus the echoed outgoing "yo".
        let sent = app.open_messages();
        assert_eq!(sent.len(), 2);
        let last = sent.last().unwrap();
        assert!(last.from_me);
        assert_eq!(last.body, "yo");
    }

    #[test]
    fn sent_message_gets_id_and_sent_status() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        // Seed a chat so it lands in chat_order and Enter can open it (post-#12).
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "hi"),
        });
        app.on_key(key(KeyCode::Enter));
        for c in "hi".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        let action = app.on_key(key(KeyCode::Enter));
        let sent = app.open_messages();
        let last = sent.last().unwrap();
        assert!(last.from_me);
        assert!(!last.id.is_empty());
        assert_eq!(last.status, DeliveryState::Sent);
        // The action carries the same id so the backend/receipts can match it.
        match action {
            Action::Send { id, chat, body } => {
                assert_eq!(id, last.id);
                assert_eq!(chat, "a@s");
                assert_eq!(body, "hi");
            }
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn receipt_advances_status_without_regressing() {
        let mut app = App::default();
        // Seed an outgoing message with a known id.
        app.messages
            .entry("a@s".into())
            .or_default()
            .push(Message::outgoing("m1", "hey"));
        app.messages.get_mut("a@s").unwrap()[0].status = DeliveryState::Sent;

        app.apply_event(BackendEvent::Receipt {
            chat: "a@s".into(),
            ids: vec!["m1".into()],
            state: DeliveryState::Read,
        });
        assert_eq!(app.messages["a@s"][0].status, DeliveryState::Read);

        // A late Delivered receipt must NOT pull it back from Read.
        app.apply_event(BackendEvent::Receipt {
            chat: "a@s".into(),
            ids: vec!["m1".into()],
            state: DeliveryState::Delivered,
        });
        assert_eq!(app.messages["a@s"][0].status, DeliveryState::Read);
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
    fn visible_sidebar_empty_query_lists_all_chats() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "1"),
        });
        app.apply_event(BackendEvent::Message {
            chat: "b@s".into(),
            msg: msg(false, "2"),
        });
        let rows = app.visible_sidebar();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.kind == Kind::Chat));
    }

    #[test]
    fn visible_sidebar_filters_chats_by_query() {
        let mut app = App::default();
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
            msg: msg(false, "1"),
        });
        app.apply_event(BackendEvent::Message {
            chat: "b@s".into(),
            msg: msg(false, "2"),
        });
        app.query = "ali".into();
        let rows = app.visible_sidebar();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Alice");
        assert_eq!(rows[0].kind, Kind::Chat);
    }

    #[test]
    fn visible_sidebar_falls_back_to_contacts_when_no_chat_matches() {
        let mut app = App::default();
        app.set_contacts(vec![
            Contact {
                jid: "a@s".into(),
                name: "Alice".into(),
            },
            Contact {
                jid: "z@s".into(),
                name: "Zara".into(),
            },
        ]);
        // Only Alice has a chat; query matches Zara (a contact, no chat).
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "1"),
        });
        app.query = "zar".into();
        let rows = app.visible_sidebar();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Zara");
        assert_eq!(rows[0].kind, Kind::Contact);
        assert_eq!(rows[0].jid, "z@s");
    }

    #[test]
    fn tab_cycle_includes_search() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        assert_eq!(app.focus, Focus::Sidebar);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Search);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Input);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn typing_in_search_edits_query_and_resets_selection() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.focus = Focus::Search;
        app.selected = 3;
        for c in "ali".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.query, "ali");
        assert_eq!(app.selected, 0);
        app.on_key(key(KeyCode::Backspace));
        assert_eq!(app.query, "al");
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn tab_cycles_sidebar_search_and_input() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        assert_eq!(app.focus, Focus::Sidebar);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Search);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Input);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn esc_from_sidebar_quits() {
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        assert_eq!(app.on_key(key(KeyCode::Esc)), Action::Quit);
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
    fn presence_event_is_stored() {
        use crate::backend::Presence;
        let mut app = App::default();
        app.apply_event(BackendEvent::Presence {
            chat: "a@s".into(),
            state: Presence::Typing,
        });
        assert_eq!(app.presence.get("a@s"), Some(&Presence::Typing));
    }

    #[test]
    fn presence_label_reflects_open_chat() {
        use crate::backend::Presence;
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "A".into(),
        }]);
        // Seed the chat so it appears in `chat_order`, then open it.
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: msg(false, "x"),
        });
        app.on_key(key(KeyCode::Enter)); // open a@s
        assert_eq!(app.presence_label(), None);
        app.apply_event(BackendEvent::Presence {
            chat: "a@s".into(),
            state: Presence::Typing,
        });
        assert_eq!(app.presence_label().as_deref(), Some("typing…"));
        app.apply_event(BackendEvent::Presence {
            chat: "a@s".into(),
            state: Presence::Online,
        });
        assert_eq!(app.presence_label().as_deref(), Some("online"));
        app.apply_event(BackendEvent::Presence {
            chat: "a@s".into(),
            state: Presence::Offline {
                last_seen: Some("today 14:05".into()),
            },
        });
        assert_eq!(
            app.presence_label().as_deref(),
            Some("last seen today 14:05")
        );
    }

    #[test]
    fn quit_keys_request_quit() {
        let mut app = App::default();
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), Action::Quit);
        app.apply_event(BackendEvent::Connected);
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), Action::Quit);
    }
}
