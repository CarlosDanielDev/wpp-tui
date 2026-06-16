use anyhow::Result;
use async_trait::async_trait;

/// One contact / chat in the list. Rendered in the P3 contacts phase.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub jid: String,
    pub name: String,
}

/// A text message in a conversation.
#[derive(Debug, Clone)]
pub struct Message {
    pub from_me: bool,
    pub body: String,
}

/// A contact's presence in a chat.
// `Online`/`Offline` are only constructed by the whatsmeow FFI path and the
// presence-label tests; the mock seeds only `Typing`, so the default build
// would otherwise flag them as never-constructed.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum Presence {
    /// Composing a message right now.
    Typing,
    /// Online (no last-seen needed).
    Online,
    /// Offline; `last_seen` is a human string if the contact shares it.
    Offline { last_seen: Option<String> },
}

/// An event pushed from the backend up to the app.
#[derive(Debug, Clone)]
pub enum BackendEvent {
    /// QR string to render for pairing.
    Qr(String),
    /// Pairing / login succeeded.
    Connected,
    /// Incoming message for a chat.
    Message { chat: String, msg: Message },
    /// A presence update for a chat.
    Presence { chat: String, state: Presence },
}

/// Transport abstraction. The real implementation talks to whatsmeow over FFI;
/// the mock implementation simulates everything for TUI development.
#[async_trait]
pub trait Backend: Send + Sync {
    /// Begin connecting / pairing.
    async fn connect(&self) -> Result<()>;
    /// Fetch the contact / recent-chat list. Consumed in the P3 contacts phase.
    #[allow(dead_code)]
    async fn contacts(&self) -> Result<Vec<Contact>>;
    /// Send a text message to a chat. Consumed in the P4 chat phase.
    #[allow(dead_code)]
    async fn send(&self, chat: &str, body: &str) -> Result<()>;
    /// Await the next backend event (long-poll).
    async fn next_event(&self) -> Result<BackendEvent>;
}

pub mod mock;
pub use mock::MockBackend;

#[cfg(feature = "whatsmeow")]
pub mod whatsmeow;
#[cfg(feature = "whatsmeow")]
pub use whatsmeow::WhatsmeowBackend;
