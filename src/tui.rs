//! Terminal lifecycle: raw mode + alternate screen, with guaranteed restore on
//! panic or exit so the user's terminal is left usable.

use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

/// The concrete terminal type the app draws to.
pub type Term = Terminal<CrosstermBackend<Stdout>>;

/// Enter raw mode + the alternate screen and install a panic hook that restores
/// the terminal before the default hook prints the panic message.
pub fn init() -> Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    set_panic_hook();
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok(terminal)
}

/// Leave the alternate screen and disable raw mode. Safe to call more than once.
pub fn restore() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

/// Wrap the existing panic hook so a panic restores the terminal first; without
/// this a panic in raw mode leaves the user's shell unusable.
fn set_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        hook(info);
    }));
}
