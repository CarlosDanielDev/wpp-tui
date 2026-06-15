//! Real WhatsApp backend: drives the Go/whatsmeow c-archive over FFI.
//!
//! Only compiled under the `whatsmeow` feature. `connect` opens the client and
//! starts pairing; `next_event` polls the Go side and translates its state into
//! [`BackendEvent`]s. Contacts and outbound/inbound messages are still stubs —
//! those land with the contacts (#9) and chat-core (#11) phases, when the Go
//! surface grows the matching exports.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;

use super::{Backend, BackendEvent, Contact};
use crate::bridge;

/// Directory holding the whatsmeow SQLite session store. Overridable so several
/// instances (or tests) don't fight over one device file.
fn session_dir() -> String {
    std::env::var("WPP_DATA_DIR").unwrap_or_else(|_| "wpp-data".to_string())
}

/// Build an error from a Go status code, attaching the bridge's last message.
fn bridge_err(op: &str, code: i32) -> anyhow::Error {
    match bridge::last_error() {
        Some(msg) => anyhow!("{op} failed (code {code}): {msg}"),
        None => anyhow!("{op} failed (code {code})"),
    }
}

pub struct WhatsmeowBackend {
    /// Ensures the one-shot `Connected` event is emitted exactly once even
    /// though `is_connected` stays true for the life of the session.
    connected_emitted: AtomicBool,
}

impl Default for WhatsmeowBackend {
    fn default() -> Self {
        Self {
            connected_emitted: AtomicBool::new(false),
        }
    }
}

impl Drop for WhatsmeowBackend {
    fn drop(&mut self) {
        bridge::disconnect();
    }
}

#[async_trait]
impl Backend for WhatsmeowBackend {
    async fn connect(&self) -> Result<()> {
        // `init` opens SQLite and `start` may block on network setup, so run the
        // pair of blocking FFI calls off the async runtime's worker threads.
        tokio::task::spawn_blocking(|| {
            let dir = session_dir();
            std::fs::create_dir_all(&dir)
                .map_err(|e| anyhow!("create session dir {dir:?}: {e}"))?;
            bridge::init(&dir).map_err(|code| bridge_err("wpp_bridge_init", code))?;
            bridge::start().map_err(|code| bridge_err("wpp_bridge_start", code))?;
            Ok::<(), anyhow::Error>(())
        })
        .await??;
        Ok(())
    }

    async fn contacts(&self) -> Result<Vec<Contact>> {
        // Contact retrieval lands with issue #9.
        Ok(vec![])
    }

    async fn send(&self, _chat: &str, _body: &str) -> Result<()> {
        // Outbound messaging lands with issue #11.
        Ok(())
    }

    async fn next_event(&self) -> Result<BackendEvent> {
        // The poll calls are cheap, non-blocking atomic/queue reads on the Go
        // side, so they're fine to call directly between async sleeps.
        loop {
            if let Some(code) = bridge::poll_qr() {
                return Ok(BackendEvent::Qr(code));
            }
            if bridge::is_connected() && !self.connected_emitted.swap(true, Ordering::SeqCst) {
                return Ok(BackendEvent::Connected);
            }
            if let Some(err) = bridge::last_error() {
                return Err(anyhow!("whatsmeow bridge error: {err}"));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}
