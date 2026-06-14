//! Reusable DOS-style TUI chrome (frame + F-key status bar) and 16-color
//! palette.
//!
//! Self-contained so the widget can be rendered and unit-tested without the
//! full event loop. Consumed by the P1 TUI shell (screen routing), which wires
//! the status-bar F-keys to real actions.
// Public API consumed by the TUI shell (issue-1); not yet wired into the
// binary, so suppress the bin-crate "unused" lints until the shell lands.
#![allow(dead_code, unused_imports)]

pub mod chrome;
pub mod theme;

pub use chrome::{Chrome, FKey, Screen};
