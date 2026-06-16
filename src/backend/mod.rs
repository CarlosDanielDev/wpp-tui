use anyhow::Result;
use async_trait::async_trait;

/// One contact / chat in the list. Rendered in the P3 contacts phase.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub jid: String,
    pub name: String,
}

/// Delivery state of an outgoing message. Ordered so a receipt never regresses
/// the status (see `rank`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryState {
    Sending,
    Sent,
    Delivered,
    Read,
}

impl DeliveryState {
    /// Monotonic rank — higher is further along. Used to guard against
    /// out-of-order receipts moving a message backwards.
    pub fn rank(self) -> u8 {
        match self {
            DeliveryState::Sending => 0,
            DeliveryState::Sent => 1,
            DeliveryState::Delivered => 2,
            DeliveryState::Read => 3,
        }
    }
}

/// A text message in a conversation.
#[derive(Debug, Clone)]
pub struct Message {
    /// Server / local message id. Empty for incoming messages without one.
    pub id: String,
    pub from_me: bool,
    pub body: String,
    /// Delivery state. Only meaningful for `from_me` messages.
    pub status: DeliveryState,
}

impl Message {
    /// An incoming message (no id needed, state irrelevant).
    pub fn incoming(body: impl Into<String>) -> Self {
        Self {
            id: String::new(),
            from_me: false,
            body: body.into(),
            status: DeliveryState::Sent,
        }
    }

    /// An outgoing message starting in `Sending`.
    pub fn outgoing(id: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            from_me: true,
            body: body.into(),
            status: DeliveryState::Sending,
        }
    }
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
    /// Delivery receipt(s) advancing message status for a chat.
    Receipt {
        chat: String,
        ids: Vec<String>,
        state: DeliveryState,
    },
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
