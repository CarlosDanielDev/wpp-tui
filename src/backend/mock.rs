use std::collections::VecDeque;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Mutex;

use super::{Backend, BackendEvent, Contact, Message};

/// Simulated WhatsApp backend. Lets the TUI/UX be built and tested without a
/// live account or the Go FFI layer.
pub struct MockBackend {
    events: Mutex<VecDeque<BackendEvent>>,
}

impl Default for MockBackend {
    fn default() -> Self {
        let mut events = VecDeque::new();
        events.push_back(BackendEvent::Qr("MOCK-QR-SCAN-ME".to_string()));
        events.push_back(BackendEvent::Connected);
        events.push_back(BackendEvent::Message {
            chat: "5511999990000@s.whatsapp.net".to_string(),
            msg: Message {
                from_me: false,
                body: "hello from the mock backend".to_string(),
            },
        });
        Self {
            events: Mutex::new(events),
        }
    }
}

#[async_trait]
impl Backend for MockBackend {
    async fn connect(&self) -> Result<()> {
        Ok(())
    }

    async fn contacts(&self) -> Result<Vec<Contact>> {
        Ok(vec![
            Contact {
                jid: "5511999990000@s.whatsapp.net".into(),
                name: "Alice (mock)".into(),
            },
            Contact {
                jid: "5511888880000@s.whatsapp.net".into(),
                name: "Bob (mock)".into(),
            },
        ])
    }

    async fn send(&self, _chat: &str, _body: &str) -> Result<()> {
        Ok(())
    }

    async fn next_event(&self) -> Result<BackendEvent> {
        loop {
            if let Some(event) = self.events.lock().await.pop_front() {
                return Ok(event);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_yields_seeded_events_then_contacts() {
        let backend = MockBackend::default();
        backend.connect().await.unwrap();

        assert!(matches!(
            backend.next_event().await.unwrap(),
            BackendEvent::Qr(_)
        ));
        assert!(matches!(
            backend.next_event().await.unwrap(),
            BackendEvent::Connected
        ));
        assert_eq!(backend.contacts().await.unwrap().len(), 2);
    }
}
