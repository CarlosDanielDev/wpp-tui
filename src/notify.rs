//! Desktop notifications for incoming messages. The *decision* (whether to
//! notify and with what text) is pure and unit-tested; firing it is an OS call.

use crate::app::{App, Focus};
use crate::backend::BackendEvent;

/// Notification text for an event, or `None` if no notification should fire.
/// Fires only for an incoming message in a chat the user is NOT currently
/// looking at (i.e. it would land as unread).
pub fn notify_text(app: &App, event: &BackendEvent) -> Option<String> {
    let BackendEvent::Message { chat, msg } = event else {
        return None;
    };
    if msg.from_me {
        return None;
    }
    let focused = app.focus == Focus::Input && app.open_chat.as_deref() == Some(chat.as_str());
    if focused {
        return None;
    }
    let who = app
        .contacts
        .iter()
        .find(|c| &c.jid == chat)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| chat.clone());
    Some(format!("{who}: {}", msg.body))
}

/// Fire a desktop notification. Best-effort: failures are ignored.
pub fn fire(text: &str) {
    #[cfg(target_os = "macos")]
    {
        let script = format!("display notification {text:?} with title \"wpp-tui\"");
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .arg("wpp-tui")
            .arg(text)
            .spawn();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = text;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Contact, Message};

    #[test]
    fn notifies_for_unfocused_incoming() {
        let mut app = App::default();
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "Alice".into(),
        }]);
        let ev = BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message::incoming("hi"),
        };
        assert_eq!(notify_text(&app, &ev).as_deref(), Some("Alice: hi"));
    }

    #[test]
    fn no_notify_for_focused_chat() {
        use crossterm::event::{KeyCode, KeyEvent};
        let mut app = App::default();
        app.apply_event(BackendEvent::Connected);
        app.set_contacts(vec![Contact {
            jid: "a@s".into(),
            name: "Alice".into(),
        }]);
        // Seed the chat so it lands in chat_order, then open it (focus → Input).
        app.apply_event(BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message::incoming("seed"),
        });
        app.on_key(KeyEvent::from(KeyCode::Enter)); // open a@s
        let ev = BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message::incoming("hi"),
        };
        assert_eq!(notify_text(&app, &ev), None);
    }

    #[test]
    fn no_notify_for_own_message() {
        let app = App::default();
        let ev = BackendEvent::Message {
            chat: "a@s".into(),
            msg: Message::outgoing("m1", "yo"),
        };
        assert_eq!(notify_text(&app, &ev), None);
    }
}
