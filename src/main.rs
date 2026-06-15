mod app;
mod backend;
#[cfg(feature = "whatsmeow")]
mod bridge;
mod tui;
mod ui;
mod widgets;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use tokio::sync::mpsc;

use app::{Action, App};
use backend::{Backend, BackendEvent, MockBackend};
use tui::Term;

/// Things the event loop reacts to: backend pushes and user keystrokes,
/// multiplexed onto one channel so the loop is a single `recv` await.
enum Tick {
    Backend(BackendEvent),
    Key(crossterm::event::KeyEvent),
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

    let backend = Arc::new(MockBackend::default());
    backend.connect().await?;

    let mut app = App::default();
    app.set_contacts(backend.contacts().await?);

    let mut terminal = tui::init()?;
    let result = run(&mut terminal, &mut app, backend).await;
    tui::restore()?;
    result
}

async fn run(terminal: &mut Term, app: &mut App, backend: Arc<MockBackend>) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<Tick>(64);

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
    drop(tx);

    // Initial paint before the first event arrives.
    terminal.draw(|f| ui::draw(f, app))?;

    while let Some(tick) = rx.recv().await {
        match tick {
            Tick::Backend(ev) => app.apply_event(ev),
            Tick::Key(key) => match app.on_key(key) {
                Action::None => {}
                Action::Quit => {
                    app.should_quit = true;
                }
                Action::Send { chat, body } => {
                    backend.send(&chat, &body).await?;
                }
            },
        }

        terminal.draw(|f| ui::draw(f, app))?;
        if app.should_quit {
            break;
        }
    }

    Ok(())
}
