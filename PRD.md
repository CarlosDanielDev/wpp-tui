# wpp-tui — Product Requirements Document

> A DOS-style terminal WhatsApp client. Public experiment. Not monetized, not a
> replacement for WhatsApp. Built for fun.

## Vision

Log into a personal WhatsApp account by scanning a QR code rendered in the
terminal, then browse contacts and hold a one-to-one text conversation — all
inside a blocky, 16-color, F-key-driven DOS-style TUI.

## Non-goals

- Group chats, channels, communities, status/stories.
- Calls (voice/video), payments, business features.
- Media composing (sending images/audio/docs). Incoming media is out of scope
  for v1 beyond a textual placeholder.
- Multi-account. One paired device at a time.
- Being a stable or complete WhatsApp client. This is an experiment and will
  break when the protocol changes.

## Users

Tinkerers and terminal lovers who want a retro chat experience and don't mind a
rough, experimental tool.

## Stack

- **Rust** + **Tokio** — app core, async event loop.
- **Ratatui** + **crossterm** — terminal UI.
- **whatsmeow** (Go) — WhatsApp multidevice protocol: Noise/Signal crypto, QR
  pairing, session store. Compiled `go build -buildmode=c-archive` and linked
  into the Rust binary via a cgo C-ABI FFI shim. **Single binary.**

There is no official WhatsApp client API for personal accounts; QR-login means
speaking the multidevice protocol, which whatsmeow already implements.

## Architecture

```
┌─ TUI (ratatui)  ── screens: QR-login → contact list → chat view
├─ Core (tokio)   ── app state, event loop, async channels, message cache
└─ Bridge (FFI)   ── Rust extern "C"  ⇄  Go c-archive shim ⇄ whatsmeow
                     whatsmeow owns: Noise/Signal crypto, QR pair, SQLite store
```

### Backend trait

The transport is hidden behind a `Backend` trait with two implementations:

- **`MockBackend`** — simulates QR pairing, contacts, and messages. Lets the
  TUI/UX be built and tested with no live account and no Go toolchain. Default
  build.
- **`WhatsmeowBackend`** — real transport over FFI. Enabled by the `whatsmeow`
  Cargo feature; requires a Go toolchain at build time.

This isolates the hard, fragile FFI work from the UI work and keeps the default
build pure-Rust and green.

### Data flow

```
whatsmeow event → C callback → tokio mpsc → app state update → ratatui redraw
keystroke → command → FFI call → whatsmeow → wire
```

### Persistence

whatsmeow's own SQLite store holds session keys, so a paired device survives
restarts without re-scanning. An app-side cache holds contacts and recent
messages.

### Error handling

The FFI boundary returns status codes plus error strings; the bridge layer maps
them to Rust `Result`. Disconnects trigger a backoff reconnect loop. QR expiry
regenerates the code.

## Functional requirements

| ID | Requirement |
|----|-------------|
| F1 | Render a scannable QR code in the terminal for device pairing. |
| F2 | Persist the paired session; reconnect on restart without re-scan. |
| F3 | Fetch and display a contact / recent-chat list. |
| F4 | Open a contact and view message history. |
| F5 | Send a text message; see it appear in the conversation. |
| F6 | Receive incoming text messages in real time. |
| F7 | Show typing indicators and online/last-seen presence. |
| F8 | Show read receipts (sent / delivered / read). |
| F9 | DOS-style UI: box-drawing borders, F-key status bar, 16-color palette. |

## Phases

Each phase maps to a milestone; each task maps to a GitHub issue
(`maestro:ready` + priority label, dependencies via `blocked-by:#N`).

- **P0 — Scaffold.** Cargo project, `Backend` trait + `MockBackend`, Go cgo
  c-archive skeleton (feature-gated), `build.rs`, CI. Compiles and runs against
  the mock. No real WhatsApp.
- **P1 — TUI shell.** Ratatui DOS layout, F-key status bar, screen routing
  (login / contacts / chat), tokio event loop. Drives the mock backend → fully
  playable.
- **P2 — QR login.** whatsmeow pairing over FFI, QR rendered in the terminal,
  session persisted, reconnect loop.
- **P3 — Contacts.** Pull contact / chat list from whatsmeow, render list pane,
  select to open a chat.
- **P4 — Chat core.** Send/receive text, load history, render the message pane.
- **P5 — Presence.** Typing indicators, online/last-seen, read receipts.
- **P6 — Polish.** Notifications, theming, error overlays, docs.

## Success criteria

Scan the QR with a phone, see your contacts, open one, exchange text messages
with typing and read indicators — inside a DOS-style terminal UI, from a single
binary.

## Legal / ethical

Unofficial, reverse-engineered protocol use. Educational experiment only. Use at
your own risk; may violate WhatsApp's Terms of Service. No warranty.
