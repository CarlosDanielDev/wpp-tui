# wpp-tui

> A DOS-style terminal WhatsApp client. **Public experiment — for fun.** Not
> monetized, not a replacement for WhatsApp, and not affiliated with WhatsApp or
> Meta.

Log into a personal WhatsApp account by scanning a QR code in your terminal,
then browse contacts and chat (text) inside a blocky, F-key-driven DOS-style UI.

## Stack

- **Rust + Tokio** — app core and async event loop
- **Ratatui + crossterm** — terminal UI
- **whatsmeow (Go)** — WhatsApp multidevice protocol, linked via a cgo
  c-archive FFI shim (single binary)

See [PRD.md](PRD.md) for the full design and phase plan.

## Status

Early scaffold. The default build runs against a **mock backend** (no real
WhatsApp) so the UI can be developed without a live account:

```bash
cargo run
```

The real transport lands behind the `whatsmeow` Cargo feature (requires a Go
toolchain) in the QR-login phase.

## Architecture

```
┌─ TUI (ratatui)  ── login → contacts → chat
├─ Core (tokio)   ── state, event loop, message cache
└─ Bridge (FFI)   ── Rust extern "C" ⇄ Go c-archive ⇄ whatsmeow
```

The transport sits behind a `Backend` trait with two implementations:
`MockBackend` (default) and `WhatsmeowBackend` (real, feature-gated).

## Usage

### Build & run (mock backend — no account, no Go toolchain)

```bash
cargo run            # simulated WhatsApp: seeded QR, contacts, a message
```

### Build & run (real WhatsApp via whatsmeow)

Requires a Go toolchain.

```bash
cargo run --features whatsmeow           # scan the QR with WhatsApp → Linked Devices
cargo run --features whatsmeow -- --mock # force the mock backend even with the feature on
```

### Keys

- Contacts: `↑↓`/`jk` move · `Enter` open · `r`/`F5` refresh · `q`/`Esc` quit
- Search: `Tab` from the sidebar focuses the Search box; type to fuzzy-filter,
  `Enter`/`Tab` move on, `Esc` back to the chat list
- Chat: type to compose · `Enter` send · `Tab`/`Esc` back to the sidebar

### Config

- `WPP_THEME=retro|amber` — colour theme (default `retro`)
- `WPP_DATA_DIR=path` — session + message store location (default `wpp-data`)

### Notifications

Incoming messages for chats you're not viewing fire a desktop notification
(`osascript` on macOS, `notify-send` on Linux).

## Development

This project is built phase-by-phase via GitHub issues, orchestrated with
[maestro](https://github.com/CarlosDanielDev/maestro).

## Legal

Unofficial, reverse-engineered protocol use for educational purposes. May
violate WhatsApp's Terms of Service. Use at your own risk. No warranty. MIT
licensed.
