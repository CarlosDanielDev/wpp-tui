# Two-pane chat layout + chat-list sidebar + fuzzy search — Design

**Date:** 2026-06-15
**Status:** Approved (brainstorming) — ready for implementation planning.

## Goal

Replace the full-screen routed UI (Login → Contacts → Chat) with a persistent
two-pane layout after pairing: a search box + chat-list sidebar on the left, and
a chat pane (or an empty placeholder) on the right. Load **chats**, not the full
contacts list. Search fuzzy-filters chats; when no chat matches, it falls back to
fuzzy-filtering contacts, and selecting a contact starts a new conversation.

## Reference mockups (user-supplied)

- **Idle:** left = search header box above a chat-list sidebar (~6 rows); right =
  empty hatched pane (no chat selected).
- **Chat open:** right pane gains a header line (chat title), alternating
  left/right message rows, and an input box at the bottom.
- **Search active:** search box shows the query; sidebar narrows to matching rows;
  right pane shows the open chat.

## Decisions (locked during brainstorming)

1. **Chat list source:** union of persisted-store chats (issue #13 `FileStore`) and
   any chat that receives a live incoming/outgoing message this session. Sorted by
   last activity (most recent first). Fresh install = empty sidebar until a message
   arrives or a chat is started via search.
2. **Navigation:** Tab / Shift-Tab cycle focus Search → Sidebar → Input. Esc:
   Input → Sidebar; Sidebar/Search → quit (with confirm). No new full screens.
3. **Search:** fuzzy-filter chats live; **zero chat matches → fuzzy-filter
   contacts**; Enter on a contact result opens a blank chat for that JID (start a
   new conversation). Enter on a chat result opens the existing chat.
4. **Fuzzy matching:** hand-rolled subsequence scorer, no external crate.

## Architecture

### State model (`src/app.rs`)

- `Screen` shrinks from `{ Login, Contacts, Chat }` to **`{ Login, Main }`**.
  - `Login` — QR pairing, unchanged (full screen until `BackendEvent::Connected`).
  - `Main` — the persistent two-pane layout.
- **New `Focus { Search, Sidebar, Input }`** — which region has keyboard focus on
  the Main screen. Default after connect = `Sidebar`.
- `open_chat: Option<String>` (existing) drives the right pane: `None` → empty
  hatched placeholder; `Some(jid)` → chat pane.
- **New `query: String`** — the search box contents.
- **New `chat_order: Vec<String>`** — sidebar order: JIDs with store history ∪
  live-session JIDs, sorted by last activity (newest first).
- **New `selected` semantics** — index into the *currently visible* sidebar rows
  (see `visible_sidebar`), not into `contacts`.
- Retained: `contacts`, `messages`, `unread`, `presence`, `status`, `theme`,
  `overlay`, `should_quit`, `tick`, plus #15's `msg_seq`/`history_loaded`.

### Layout (`src/ui.rs`)

`draw` for `Screen::Main` splits horizontally:

- **Left column** (fixed width, e.g. 32 cols): vertical split → search box (height
  3) over the chat-list sidebar (`Min(1)`).
- **Right column** (`Min`): if `open_chat.is_none()` → empty placeholder block
  (hatched/idle); else the chat pane = header line + history (`visible_tail` from
  #12) + input box.
- The focused region gets the bright `accent_dk` border; unfocused regions use the
  dim border. Status bar spans the bottom as today; disconnect overlay (#16) floats
  centred over everything.

### Sidebar derivation (pure)

```
enum Kind { Chat, Contact }
struct Row { jid: String, name: String, unread: usize, kind: Kind }

fn visible_sidebar(app: &App) -> Vec<Row>
```

1. `query` empty → every JID in `chat_order` as `Kind::Chat` rows (name from
   `contacts`, fallback to JID; unread from `app.unread`).
2. `query` non-empty → keep `chat_order` rows whose name-or-JID has
   `fuzzy_score(query, hay).is_some()`, sorted by descending score.
3. If step 2 yields **zero** rows → fuzzy-filter `contacts` instead, emit
   `Kind::Contact` rows sorted by score.

`selected` indexes the returned `Vec<Row>`; clamped on every query change.

### Fuzzy core (pure, `src/fuzzy.rs`)

```
fn fuzzy_score(needle: &str, haystack: &str) -> Option<i32>
```

- Case-insensitive subsequence test: every char of `needle` appears in order in
  `haystack`. `None` if not a subsequence.
- Score rewards contiguous runs and a match at the start of `haystack`; higher is
  better. Empty needle scores 0 (matches everything).
- Unit-tested in isolation; no I/O, no crate.

### Key handling (`src/app.rs::on_key`, Main screen)

Dispatch by `focus`:

- **Search:** printable chars / Backspace edit `query` and reset `selected = 0`;
  Tab → Sidebar; Esc → quit (confirm).
- **Sidebar:** `j/k`/`↑↓` move `selected` (clamped to `visible_sidebar` len);
  Enter → open the selected row's chat (`open_chat = Some(jid)`, clear its unread,
  `focus = Input`, return `Action::OpenChat { chat }` so #13 loads history; for a
  `Kind::Contact` row the loaded history is empty = blank pane); Tab → Input; Esc →
  quit (confirm).
- **Input:** printable/Backspace edit `input`; Enter → send (existing echo + id
  stamping from #15, returns `Action::Send`); Tab → Search; Esc → Sidebar.

### Chat-order maintenance

- On startup: seed `chat_order` from the store's chat index (a new
  `FileStore::list_chats() -> Vec<String>` over `wpp-data/chats/*.log`, ordered by
  file mtime; #13 extension).
- In `apply_event(Message { chat, .. })`: move `chat` to the front of `chat_order`
  (insert if absent). This is the "live merge" + last-activity sort.
- On send (`Action::Send`): the echoed message already pushes to `messages`; also
  move `chat` to the front of `chat_order` (covers contact-started chats).

## Data flow

```
startup → FileStore::list_chats() → chat_order
Connected → focus = Sidebar, open_chat = None (empty right pane)
key → on_key(focus) → mutate query/selected/input or return Action
  OpenChat → main loads store history (#13) → messages[jid]
  Send → store append + backend.send (#11/#15)
incoming Message → apply_event → messages + unread + chat_order front
draw(Main) → visible_sidebar(app) + chat pane(open_chat)
```

## Issue remapping

- **#12 (TUI chat view)** → redefined as **two-pane layout shell + chat pane +
  sidebar render**: `Screen::Main`, `Focus`, left column (search box + sidebar),
  right column (empty placeholder / chat pane), Tab/Esc focus model. Keeps the
  `visible_tail` auto-scroll work. Depends on #11, #13.
- **NEW issue (#29): search + fuzzy + contact fallback + new chat** — `src/fuzzy.rs`,
  `App.query`, `visible_sidebar`, contact-fallback, contact-row → blank chat start,
  `FileStore::list_chats`, `chat_order` maintenance. Depends on #12, #13, and the
  contacts fetch (already built).
- **#13** stays; gains `FileStore::list_chats()` (used by #12/#29 to seed
  `chat_order`).
- **#14 (presence)** — renders into the chat-pane header instead of the old
  full-screen chat header. Fold unchanged.
- **#15 (receipts)** — markers render in the chat pane. Fold unchanged.
- **#16 (polish)** — disconnect overlay floats over Main; notification proxy
  ("chat not focused") = "incoming chat ≠ open_chat OR focus ≠ Input". Theme/README
  unchanged.

Revised build order:

```
#11 ─┬─→ #13 ─┬─→ #12(layout) ─┬─→ #29(search)
     │        │                ├─→ #14 ─┐
     └────────┘                └─→ #15 ─┴─→ #16
```

## Testing strategy

All new logic has a pure, CI-testable core (the project's established pattern —
`apply_event`/`on_key` stay side-effect free; I/O lives in `main.rs`):

- `fuzzy_score` — subsequence hits/misses, ordering, start/contiguity bonus, empty
  needle.
- `visible_sidebar` — empty query lists chats; query filters chats; zero-chat
  fallback to contacts; `Kind` tagging; selected clamping.
- `chat_order` — live `Message` moves a chat to front; new chat inserted; send
  fronts a contact-started chat.
- Focus transitions — Tab cycle, Enter opens + focuses Input, Esc ladder.
- Render (`TestBackend`) — idle shows empty right pane; open chat shows pane;
  search narrows sidebar; focused region border styling.
- `FileStore::list_chats` — tempdir with N logs returns N JIDs.

## Out of scope (YAGNI)

- Group chats / multi-party rendering.
- Message search within a conversation.
- Sidebar scrolling beyond the visible window (chats list assumed to fit; revisit
  if needed).
- Mouse support.
