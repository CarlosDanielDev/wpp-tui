mod app;
mod backend;
mod tui;
mod ui;

use anyhow::Result;
use backend::{Backend, MockBackend};

/// P1 entrypoint: set up the terminal, run the TUI event loop against the mock
/// backend, then always restore the terminal — even if the loop errored.
#[tokio::main]
async fn main() -> Result<()> {
    let backend = MockBackend::default();
    backend.connect().await?;

    let mut terminal = tui::init()?;
    let result = app::run(&mut terminal, &backend).await;
    tui::restore()?;

    result
}
