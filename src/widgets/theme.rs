//! DOS 16-color palette mapped onto ratatui's named ANSI colors.
//!
//! Classic Turbo Vision look: a blue "desktop", bright white double borders,
//! a cyan F-key status bar with highlighted key numbers.

use ratatui::style::Color;

/// Frame interior / desktop background.
pub const BG: Color = Color::Blue;
/// Default body text.
pub const FG: Color = Color::Gray;
/// Box-drawing border.
pub const BORDER: Color = Color::White;

/// Title-bar text and background.
pub const TITLE_FG: Color = Color::Black;
pub const TITLE_BG: Color = Color::Gray;

/// Status-bar label text and background.
pub const STATUS_FG: Color = Color::Black;
pub const STATUS_BG: Color = Color::Cyan;
/// Highlighted "F<n>" key marker in the status bar.
pub const STATUS_KEY_FG: Color = Color::White;
pub const STATUS_KEY_BG: Color = Color::Black;

/// Accent (selections, active markers).
pub const ACCENT: Color = Color::Yellow;
