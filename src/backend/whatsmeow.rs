//! Real WhatsApp backend: drives the Go/whatsmeow c-archive over FFI.
//!
//! Only compiled under the `whatsmeow` feature. `connect` opens the client and
//! starts pairing; `next_event` polls the Go side and translates its state into
//! [`BackendEvent`]s; `contacts` fetches the contact / recent-chat list from the
//! Go-side SQLite store. Outbound/inbound messages are still stubs — those land
//! with the chat-core phase (#11).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;

use super::{Backend, BackendEvent, Contact, Message};
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

/// Parse the tab-separated contact blob returned by the Go bridge.
/// Format: `jid\tname` per line, joined by `\n`.
fn parse_contacts(raw: &str) -> Vec<Contact> {
    raw.lines()
        .filter_map(|line| {
            let (jid, name) = line.split_once('\t')?;
            Some(Contact {
                jid: jid.to_string(),
                name: name.to_string(),
            })
        })
        .collect()
}

/// Parse one incoming-message line from the Go bridge.
/// Format: `jid\tflag\tbody`, where flag is "1" if the message is from us.
/// `body` may itself contain tabs, so split only on the first two.
fn parse_incoming(raw: &str) -> Option<(String, Message)> {
    let (jid, rest) = raw.split_once('\t')?;
    let (flag, body) = rest.split_once('\t')?;
    Some((
        jid.to_string(),
        Message {
            from_me: flag == "1",
            body: body.to_string(),
        },
    ))
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
        // `fetch_contacts` calls into Go/sqlite — keep it off the async threads.
        tokio::task::spawn_blocking(|| {
            let raw = bridge::fetch_contacts()
                .ok_or_else(|| anyhow!("fetch_contacts returned null — bridge not initialised?"))?;
            Ok(parse_contacts(&raw))
        })
        .await?
    }

    async fn send(&self, chat: &str, body: &str) -> Result<()> {
        let chat = chat.to_string();
        let body = body.to_string();
        // SendMessage does network I/O on the Go side — keep it off async workers.
        tokio::task::spawn_blocking(move || {
            bridge::send_text(&chat, &body).map_err(|code| bridge_err("wpp_bridge_send_text", code))
        })
        .await??;
        Ok(())
    }

    async fn next_event(&self) -> Result<BackendEvent> {
        // The poll calls are cheap, non-blocking atomic/queue reads on the Go
        // side, so they're fine to call directly between async sleeps.
        loop {
            if let Some(code) = bridge::poll_qr() {
                return Ok(BackendEvent::Qr(code));
            }
            if let Some(raw) = bridge::poll_message() {
                if let Some((chat, msg)) = parse_incoming(&raw) {
                    return Ok(BackendEvent::Message { chat, msg });
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_contacts_decodes_tab_separated_format() {
        let raw = "5511999990000@s.whatsapp.net\tAlice\n5511888880000@s.whatsapp.net\tBob";
        let contacts = parse_contacts(raw);
        assert_eq!(contacts.len(), 2);
        assert_eq!(contacts[0].name, "Alice");
        assert_eq!(contacts[0].jid, "5511999990000@s.whatsapp.net");
        assert_eq!(contacts[1].name, "Bob");
        assert_eq!(contacts[1].jid, "5511888880000@s.whatsapp.net");
    }

    #[test]
    fn parse_contacts_handles_empty_input() {
        let contacts = parse_contacts("");
        assert!(contacts.is_empty());
    }

    #[test]
    fn parse_contacts_skips_malformed_lines() {
        let raw = "good_jid@s.whatsapp.net\tGood\nno_tab_here\nbad@s.whatsapp.net\tAlsoGood";
        let contacts = parse_contacts(raw);
        assert_eq!(contacts.len(), 2);
        assert_eq!(contacts[0].name, "Good");
        assert_eq!(contacts[1].name, "AlsoGood");
    }

    #[test]
    fn parse_contacts_handles_trailing_newline() {
        let raw = "a@s.whatsapp.net\tA\n";
        let contacts = parse_contacts(raw);
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].name, "A");
    }

    #[test]
    fn parse_incoming_decodes_jid_flag_body() {
        let (chat, msg) = parse_incoming("5511999990000@s.whatsapp.net\t0\thello").unwrap();
        assert_eq!(chat, "5511999990000@s.whatsapp.net");
        assert!(!msg.from_me);
        assert_eq!(msg.body, "hello");
    }

    #[test]
    fn parse_incoming_keeps_tabs_in_body() {
        let (_, msg) = parse_incoming("a@s.whatsapp.net\t0\ta\tb").unwrap();
        assert_eq!(msg.body, "a\tb");
    }

    #[test]
    fn parse_incoming_rejects_malformed() {
        assert!(parse_incoming("").is_none());
        assert!(parse_incoming("only_jid").is_none());
        assert!(parse_incoming("jid\t0").is_none());
    }
}
