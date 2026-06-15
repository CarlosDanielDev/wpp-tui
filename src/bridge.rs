//! FFI surface onto the Go/whatsmeow c-archive (`bridge/`).
//!
//! Only compiled under the `whatsmeow` feature, where `build.rs` has built and
//! linked `libwppbridge.a`. For now this is a single smoke binding used to prove
//! the Go archive links and is callable; the real pairing/send/receive surface
//! lands in the QR-login phase (P2).

use std::ffi::CStr;
use std::os::raw::c_char;

extern "C" {
    /// Exported by `bridge/bridge.go` via `//export wpp_bridge_version`.
    /// Returns a C string owned by the Go side (leaked; never freed here).
    fn wpp_bridge_version() -> *const c_char;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_go_stub() {
        // Mirrors the literal returned by `bridge/bridge.go`.
        assert_eq!(version(), "0.1.0-stub");
    }
}
