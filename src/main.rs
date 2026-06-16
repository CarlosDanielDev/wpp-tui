mod app;
mod backend;
#[cfg(feature = "whatsmeow")]
mod bridge;
mod fuzzy;
mod notify;
mod qr;
mod store;
mod theme;
mod tui;
mod ui;
mod widgets;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use tokio::sync::mpsc;

use app::{Action, App};
use backend::{Backend, BackendEvent, Message, MockBackend};
use tui::Term;

/// Things the event loop reacts to: backend pushes and user keystrokes,
/// multiplexed onto one channel so the loop is a single `recv` await.
enum Tick {
    Backend(BackendEvent),
    Key(crossterm::event::KeyEvent),
    /// Periodic timer that drives animations (e.g. the QR spinner).
    Anim,
}

/// P1 entrypoint: set up the terminal, run the TUI event loop against the mock
/// backend, then always restore the terminal — even if the loop errored.
#[tokio::main]
async fn main() -> Result<()> {
    // Smoke path for the Go FFI bridge: `wpp-tui --bridge-version` prints the
    // version string read out of the linked whatsmeow c-archive and exits. Used
    // to verify the archive built and links (see issue #5).
    if std::env::args().any(|a| a == "--bridge-version") {
        #[cfg(feature = "whatsmeow")]
        println!("{}", bridge::version());
        #[cfg(not(feature = "whatsmeow"))]
        println!("whatsmeow feature not enabled; rebuild with --features whatsmeow");
        return Ok(());
    }

    let backend = make_backend();
    backend.connect().await?;

    // Contacts are fetched lazily once the backend reports `Connected` (see the
    // event loop) — the real bridge has no contacts until the device is paired,
    // so fetching here would fail before the QR is ever shown.
    let mut app = App {
        theme: theme::Theme::from_env(),
        ..App::default()
    };

    let mut terminal = tui::init()?;
    let result = run(&mut terminal, &mut app, backend).await;
    tui::restore()?;
    result
}

/// Pick the transport. When built with the `whatsmeow` feature the real FFI
/// backend is the default (this is the only way to get a scannable WhatsApp QR);
/// pass `--mock` to force the simulated backend. Without the feature it is
/// always the mock.
fn make_backend() -> Arc<dyn Backend> {
    #[cfg(feature = "whatsmeow")]
    {
        if !std::env::args().any(|a| a == "--mock") {
            return Arc::new(backend::WhatsmeowBackend::default());
        }
    }
    Arc::new(MockBackend::default())
}

/// Re-fetch contacts from the backend into the app. Non-fatal: on failure the
/// list stays as-is and the error surfaces on the status line.
async fn refresh_contacts(backend: &Arc<dyn Backend>, app: &mut App) {
    match backend.contacts().await {
        Ok(contacts) => app.set_contacts(contacts),
        Err(e) => app.status = format!("Contacts unavailable: {e}"),
    }
}

async fn run(terminal: &mut Term, app: &mut App, backend: Arc<dyn Backend>) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<Tick>(64);

    let data_dir = std::env::var("WPP_DATA_DIR").unwrap_or_else(|_| "wpp-data".to_string());
    let store = store::FileStore::new(&data_dir);

    // Seed the sidebar from persisted chats so conversations survive a restart.
    // Reverse so the index order is preserved with most-recent first after
    // fronting each JID.
    if let Ok(chats) = store.list_chats() {
        for jid in chats.into_iter().rev() {
            app.front_chat_pub(&jid);
        }
    }

    // Producer: drain backend events forever.
    let event_backend = Arc::clone(&backend);
    let event_tx = tx.clone();
    tokio::spawn(async move {
        while let Ok(ev) = event_backend.next_event().await {
            if event_tx.send(Tick::Backend(ev)).await.is_err() {
                break;
            }
        }
    });

    // Producer: forward keystrokes. crossterm's read is blocking, so poll it on
    // a dedicated blocking thread and bridge into the async channel.
    let key_tx = tx.clone();
    tokio::task::spawn_blocking(move || loop {
        match event::poll(Duration::from_millis(200)) {
            Ok(true) => match event::read() {
                Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                    if key_tx.blocking_send(Tick::Key(key)).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            },
            Ok(false) => {}
            Err(_) => break,
        }
    });

    // Producer: animation heartbeat so spinners advance without user input.
    let anim_tx = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(120));
        loop {
            interval.tick().await;
            if anim_tx.send(Tick::Anim).await.is_err() {
                break;
            }
        }
    });
    drop(tx);

    // Initial paint before the first event arrives.
    terminal.draw(|f| ui::draw(f, app))?;

    while let Some(tick) = rx.recv().await {
        match tick {
            Tick::Backend(ev) => {
                if let BackendEvent::Message { chat, msg } = &ev {
                    let _ = store.append(chat, msg);
                }
                if let Some(text) = notify::notify_text(app, &ev) {
                    notify::fire(&text);
                }
                let was_connected = app.connected;
                app.apply_event(ev);
                // On the first successful pairing, pull the contact list. The
                // bridge has no contacts until connected, so this can't run any
                // earlier.
                if app.connected && !was_connected {
                    refresh_contacts(&backend, app).await;
                }
            }
            Tick::Key(key) => match app.on_key(key) {
                Action::None => {}
                Action::Quit => {
                    app.should_quit = true;
                }
                Action::Send { id, chat, body } => {
                    let _ = store.append(&chat, &Message::outgoing(id.clone(), body.clone()));
                    backend.send(&id, &chat, &body).await?;
                }
                Action::OpenChat { chat } => {
                    if let Ok(history) = store.load(&chat) {
                        app.load_history(chat, history);
                    }
                }
                Action::Refresh => refresh_contacts(&backend, app).await,
            },
            Tick::Anim => {
                app.tick();
                // Contacts sync lands a few seconds after pairing, so keep
                // re-fetching (~every 3s) while connected but still empty.
                if app.connected && app.contacts.is_empty() && app.tick.is_multiple_of(25) {
                    refresh_contacts(&backend, app).await;
                }
            }
        }

        terminal.draw(|f| ui::draw(f, app))?;
        if app.should_quit {
            break;
        }
    }

    Ok(())
}
