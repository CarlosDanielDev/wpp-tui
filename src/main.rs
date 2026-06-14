mod app;
mod backend;

use anyhow::Result;
use app::{App, Screen};
use backend::{Backend, BackendEvent, MockBackend};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// P0 scaffold entrypoint: drives the mock backend end-to-end so the project
/// compiles and runs before the TUI (P1) and real FFI (P2) land.
#[tokio::main]
async fn main() -> Result<()> {
    println!(
        "wpp-tui {} — DOS-style WhatsApp TUI (experiment)",
        env!("CARGO_PKG_VERSION")
    );

    let backend = MockBackend::default();
    backend.connect().await?;

    for contact in backend.contacts().await? {
        println!("contact: {} <{}>", contact.name, contact.jid);
    }

    // Drain the seeded mock events once so the wiring is visible end-to-end.
    for _ in 0..3 {
        match backend.next_event().await? {
            BackendEvent::Qr(code) => println!("[qr] {code}"),
            BackendEvent::Connected => println!("[connected]"),
            BackendEvent::Message { chat, msg } => {
                let dir = if msg.from_me { "→" } else { "←" };
                println!("[msg] {dir} {chat}: {}", msg.body);
            }
        }
    }

    // Round-trip a send through the backend so the command path is exercised too.
    backend
        .send("5511999990000@s.whatsapp.net", "hi from wpp-tui")
        .await?;
    println!("[sent] hi from wpp-tui");

    // Demonstrate the screen router end-to-end with a scripted sequence of
    // keypresses. The interactive event loop (P1) will feed real terminal keys
    // through the same `App::on_key` dispatch.
    let mut app = App::new();
    println!("[screen] {:?}", app.screen());
    for key in [
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), // Login -> Contacts
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), // Contacts -> Chat
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),   // Chat -> Contacts
    ] {
        app.on_key(key);
        if app.should_quit() {
            break;
        }
        println!("[screen] {:?}", app.screen());
    }
    debug_assert_eq!(app.screen(), Screen::Contacts);

    Ok(())
}
