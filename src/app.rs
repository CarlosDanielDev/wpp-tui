//! App state and the core async event loop. The loop multiplexes terminal key
//! events and backend events with `tokio::select!`, redrawing on each tick.

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::backend::{Backend, BackendEvent};
use crate::tui::Term;
use crate::ui;

/// Mutable application state shared between the event loop and the renderer.
pub struct App {
    /// While true the event loop keeps running; cleared by a quit key.
    running: bool,
    /// Short human-readable status, surfaced from the latest backend event.
    status: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            status: "connecting…".to_string(),
        }
    }
}

impl App {
    /// Current status line text.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Whether the loop should keep running.
    pub fn running(&self) -> bool {
        self.running
    }

    /// Handle a key press. `q` or `Ctrl+C` request a clean quit.
    pub fn on_key(&mut self, key: KeyEvent) {
        // Ignore key-release / repeat events that some terminals emit.
        if key.kind != KeyEventKind::Press {
            return;
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => self.running = false,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => self.running = false,
            _ => {}
        }
    }

    /// Fold a backend event into the status line.
    pub fn on_backend_event(&mut self, event: &BackendEvent) {
        self.status = match event {
            BackendEvent::Qr(code) => format!("scan this QR to pair: {code}"),
            BackendEvent::Connected => "connected".to_string(),
            BackendEvent::Message { chat, msg } => {
                let who = if msg.from_me { "you" } else { chat.as_str() };
                format!("{who}: {}", msg.body)
            }
        };
    }
}

/// Run the TUI event loop until a quit key is pressed.
///
/// Crossterm reads block their thread, so a dedicated OS thread forwards key
/// events into an mpsc channel; the loop then `select!`s that channel against
/// the backend's event stream. The reader thread is detached and dies with the
/// process on exit.
pub async fn run(terminal: &mut Term, backend: &dyn Backend) -> Result<()> {
    let mut app = App::default();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Event>(64);

    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            if tx.blocking_send(event).is_err() {
                break;
            }
        }
    });

    while app.running() {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        tokio::select! {
            maybe_event = rx.recv() => {
                match maybe_event {
                    Some(Event::Key(key)) => app.on_key(key),
                    Some(_) => {}
                    // Reader thread gone: nothing left to read, stop cleanly.
                    None => break,
                }
            }
            event = backend.next_event() => {
                app.on_backend_event(&event?);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Message;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn q_quits() {
        let mut app = App::default();
        app.on_key(press(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(!app.running());
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::default();
        app.on_key(press(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!app.running());
    }

    #[test]
    fn plain_c_does_not_quit() {
        let mut app = App::default();
        app.on_key(press(KeyCode::Char('c'), KeyModifiers::NONE));
        assert!(app.running());
    }

    #[test]
    fn other_keys_keep_running() {
        let mut app = App::default();
        app.on_key(press(KeyCode::Char('x'), KeyModifiers::NONE));
        app.on_key(press(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.running());
    }

    #[test]
    fn key_release_is_ignored() {
        let mut app = App::default();
        let mut release = press(KeyCode::Char('q'), KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        app.on_key(release);
        assert!(app.running());
    }

    #[test]
    fn backend_events_update_status() {
        let mut app = App::default();

        app.on_backend_event(&BackendEvent::Qr("X".to_string()));
        assert_eq!(app.status(), "scan this QR to pair: X");

        app.on_backend_event(&BackendEvent::Connected);
        assert_eq!(app.status(), "connected");

        app.on_backend_event(&BackendEvent::Message {
            chat: "alice".to_string(),
            msg: Message {
                from_me: false,
                body: "hi".to_string(),
            },
        });
        assert_eq!(app.status(), "alice: hi");

        app.on_backend_event(&BackendEvent::Message {
            chat: "alice".to_string(),
            msg: Message {
                from_me: true,
                body: "yo".to_string(),
            },
        });
        assert_eq!(app.status(), "you: yo");
    }
}
