//! FFI surface onto the Go/whatsmeow c-archive (`bridge/`).
//!
//! Only compiled under the `whatsmeow` feature, where `build.rs` has built and
//! linked `libwppbridge.a`. The Go side (`bridge/bridge.go`) owns the real
//! whatsmeow pairing, SQLite session store, and connection state; this module
//! is the thin, safe Rust wrapper over its C exports.
//!
//! Strings returned by the poll/error exports are heap-allocated on the Go side
//! and must be released with `wpp_bridge_free_string`; [`take_string`] copies
//! into an owned `String` and frees in one step. `wpp_bridge_version` is the one
//! exception — it leaks a static-ish string and is never freed.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

extern "C" {
    fn wpp_bridge_version() -> *const c_char;
    fn wpp_bridge_init(data_dir: *const c_char) -> c_int;
    fn wpp_bridge_start() -> c_int;
    fn wpp_bridge_poll_qr() -> *mut c_char;
    fn wpp_bridge_is_connected() -> c_int;
    fn wpp_bridge_last_error() -> *mut c_char;
    fn wpp_bridge_disconnect();
    fn wpp_bridge_free_string(s: *mut c_char);
    fn wpp_bridge_fetch_contacts() -> *mut c_char;
    fn wpp_bridge_send_text(jid: *const c_char, body: *const c_char) -> c_int;
    fn wpp_bridge_poll_message() -> *mut c_char;
}

/// Copy a Go-owned C string into an owned `String` and free the Go allocation.
/// Returns `None` for a null pointer (the Go convention for "nothing here").
///
/// # Safety
/// `ptr` must be either null or a valid pointer returned by a `bridge/` export
/// whose ownership transfers to us (i.e. we are responsible for freeing it).
unsafe fn take_string(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let owned = CStr::from_ptr(ptr).to_string_lossy().into_owned();
    wpp_bridge_free_string(ptr);
    Some(owned)
}

/// Read the bridge version string out of the linked Go archive.
///
/// # Panics
/// Panics if the Go side ever returns a null pointer (it never does).
pub fn version() -> String {
    // SAFETY: `wpp_bridge_version` returns a valid, NUL-terminated C string that
    // outlives this call (Go leaks the allocation), so the `CStr` borrow holds
    // for the duration of the copy into an owned `String`.
    let ptr = unsafe { wpp_bridge_version() };
    assert!(!ptr.is_null(), "wpp_bridge_version returned null");
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Open (or create) the whatsmeow client and its SQLite session store under
/// `data_dir`. `Err(code)` carries the Go-side status code; pair it with
/// [`last_error`] for a message.
pub fn init(data_dir: &str) -> Result<(), i32> {
    let c_dir = CString::new(data_dir).map_err(|_| -100)?;
    // SAFETY: `c_dir` is a valid NUL-terminated string that outlives the call.
    let code = unsafe { wpp_bridge_init(c_dir.as_ptr()) };
    if code == 0 {
        Ok(())
    } else {
        Err(code)
    }
}

/// Start pairing or reconnect. Returns the Go status code on success: `1` means
/// a new device is pairing (QR codes will follow), `0` means an existing
/// session reconnected directly. Negative codes are errors.
pub fn start() -> Result<i32, i32> {
    // SAFETY: no arguments; the Go side guards its own state.
    let code = unsafe { wpp_bridge_start() };
    if code >= 0 {
        Ok(code)
    } else {
        Err(code)
    }
}

/// Poll for the next QR string emitted during pairing, if any. `None` until the
/// next code is available; each code is returned at most once.
pub fn poll_qr() -> Option<String> {
    // SAFETY: the returned pointer is null or a Go-allocated C string we own.
    unsafe { take_string(wpp_bridge_poll_qr()) }
}

/// Whether the client is currently connected/paired.
pub fn is_connected() -> bool {
    // SAFETY: no arguments; reads an atomic on the Go side.
    unsafe { wpp_bridge_is_connected() != 0 }
}

/// The most recent Go-side error message, if one was recorded.
pub fn last_error() -> Option<String> {
    // SAFETY: the returned pointer is null or a Go-allocated C string we own.
    unsafe { take_string(wpp_bridge_last_error()) }
}

/// Tear down the client and cancel any in-flight pairing.
pub fn disconnect() {
    // SAFETY: no arguments; idempotent on the Go side.
    unsafe { wpp_bridge_disconnect() }
}

/// Fetch the contact/chat list from the Go-side whatsmeow store.
/// Returns `None` if the bridge is not initialised or an error occurred;
/// `Some("")` means the store returned an empty contact list.
pub fn fetch_contacts() -> Option<String> {
    // SAFETY: the returned pointer is null or a Go-allocated C string we own.
    unsafe { take_string(wpp_bridge_fetch_contacts()) }
}

/// Send `body` as a text message to `jid`. `Err(code)` carries the Go status
/// code; pair with [`last_error`] for a message.
pub fn send_text(jid: &str, body: &str) -> Result<(), i32> {
    let c_jid = CString::new(jid).map_err(|_| -100)?;
    let c_body = CString::new(body).map_err(|_| -100)?;
    // SAFETY: both strings are valid NUL-terminated and outlive the call.
    let code = unsafe { wpp_bridge_send_text(c_jid.as_ptr(), c_body.as_ptr()) };
    if code == 0 {
        Ok(())
    } else {
        Err(code)
    }
}

/// Poll for the next queued incoming message line (`jid\tflag\tbody`), if any.
/// `None` when the queue is empty.
pub fn poll_message() -> Option<String> {
    // SAFETY: the returned pointer is null or a Go-allocated C string we own.
    unsafe { take_string(wpp_bridge_poll_message()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_go_bridge() {
        // Mirrors the literal returned by `bridge/bridge.go`.
        assert_eq!(version(), "0.2.0");
    }

    #[test]
    fn fetch_contacts_returns_none_when_not_initialised() {
        // The Go-side client is nil in test — bridge returns null → `None`.
        assert!(fetch_contacts().is_none());
    }
}
