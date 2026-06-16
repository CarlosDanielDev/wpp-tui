//! Per-chat message persistence. One file per chat JID under the data dir,
//! one message per line as `flag\tbody` (flag "1" = from me). Bodies have
//! newlines escaped as `\n` so the format stays line-oriented.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::backend::{DeliveryState, Message};

/// File-backed per-chat message store. Each chat JID maps to one file under
/// `root/chats/`; the JID is sanitised so it is a safe filename.
pub struct FileStore {
    root: PathBuf,
}

fn sanitise(jid: &str) -> String {
    jid.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn escape(body: &str) -> String {
    body.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl FileStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().join("chats"),
        }
    }

    fn path_for(&self, jid: &str) -> PathBuf {
        self.root.join(format!("{}.log", sanitise(jid)))
    }

    /// Append one message to the chat's log, creating the directory/file.
    pub fn append(&self, jid: &str, msg: &Message) -> Result<()> {
        use std::io::Write;
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("create store dir {:?}", self.root))?;
        let flag = if msg.from_me { '1' } else { '0' };
        let line = format!("{flag}\t{}\n", escape(&msg.body));
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path_for(jid))
            .with_context(|| format!("open log for {jid}"))?;
        f.write_all(line.as_bytes())?;
        Ok(())
    }

    /// Load all stored messages for a chat (empty if none on disk).
    pub fn load(&self, jid: &str) -> Result<Vec<Message>> {
        let path = self.path_for(jid);
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).with_context(|| format!("read log for {jid}")),
        };
        Ok(raw
            .lines()
            .filter_map(|line| {
                let (flag, body) = line.split_once('\t')?;
                Some(Message {
                    id: String::new(),
                    from_me: flag == "1",
                    body: unescape(body),
                    status: DeliveryState::Sent,
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{DeliveryState, Message};

    fn msg(from_me: bool, body: &str) -> Message {
        Message {
            id: String::new(),
            from_me,
            body: body.to_string(),
            status: DeliveryState::Sent,
        }
    }

    #[test]
    fn append_then_load_roundtrips() {
        let dir = std::env::temp_dir().join(format!("wpp-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = FileStore::new(&dir);
        store.append("a@s", &msg(true, "hello")).unwrap();
        store.append("a@s", &msg(false, "hi there")).unwrap();
        let loaded = store.load("a@s").unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(loaded[0].from_me);
        assert_eq!(loaded[0].body, "hello");
        assert!(!loaded[1].from_me);
        assert_eq!(loaded[1].body, "hi there");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_chat_is_empty() {
        let dir = std::env::temp_dir().join(format!("wpp-store-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = FileStore::new(&dir);
        assert!(store.load("nope@s").unwrap().is_empty());
    }

    #[test]
    fn newlines_in_body_survive_roundtrip() {
        let dir = std::env::temp_dir().join(format!("wpp-store-nl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = FileStore::new(&dir);
        store.append("a@s", &msg(false, "line1\nline2")).unwrap();
        let loaded = store.load("a@s").unwrap();
        assert_eq!(loaded[0].body, "line1\nline2");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
