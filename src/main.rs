mod app;
mod cast;
mod config;
mod event;
mod github;
mod state;
mod theme;
mod ui;

use anyhow::Result;
use app::App;
use clap::Parser;
use crossterm::{
    cursor::Show,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;

/// Holds a reference to stdout so the terminal can be restored on drop.
///
/// By constructing this guard _after_ entering raw mode / alternate screen,
/// we guarantee that `disable_raw_mode`, `LeaveAlternateScreen`,
/// `DisableMouseCapture`, and `Show` (show cursor) are called even if the app
/// panics or returns an error mid-run.
///
/// In Rust, `Drop` is guaranteed to run when the value goes out of scope
/// (similar to Python's context managers, but automatic — no `with` needed).
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Intentionally ignore errors here — we're in a cleanup path and there
        // is nothing useful to do if these fail.
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture, Show,);
    }
}

/// octopeek — a keyboard-driven TUI for your GitHub PR and issue inbox.
#[derive(Parser, Debug)]
#[command(
    name = "octopeek",
    about = "A fast, keyboard-driven TUI for your GitHub PR and issue inbox.",
    version
)]
struct Cli {}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments (no positional arguments in Phase 1).
    let _cli = Cli::parse();

    // Initialise tracing with an environment-variable filter. Default level is
    // `warn` so production runs are quiet; set `RUST_LOG=debug` for verbose
    // output.  Uses `tracing_subscriber`'s built-in fmt layer which writes to
    // stderr, leaving stdout free for crossterm.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    // Load config and session from disk (graceful fallback to defaults).
    let config = config::Config::load();
    let session = state::AppSession::load();

    // Enter the alternate screen and raw mode. The `TerminalGuard` RAII value
    // ensures these are reversed on exit — even if the code below panics.
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // The guard must be created _after_ entering raw mode / alternate screen.
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new(config, session);
    app.run(&mut terminal).await
}
