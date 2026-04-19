//! # octopeek
//!
//! A fast, keyboard-driven terminal UI for the GitHub PR and issue inbox
//! across the repositories you care about.
//!
//! `octopeek` is a **binary crate** distributed via `cargo install octopeek`.
//! Launching it opens a ratatui-rendered dashboard that:
//!
//! - groups every open PR / issue by repository tab (one tab per repo),
//! - flags items needing the viewer's action (conflicts, failing CI,
//!   review requested, change-requests) with a single-glyph summary,
//! - supports vim-like keyboard navigation and mouse where available,
//! - renders PR bodies, review threads, and file diffs with markdown +
//!   syntax highlighting.
//!
//! This crate is **not** a library — the public API is empty. Every module
//! below is `pub(crate)` and may change between releases.
//!
//! ## Module layout
//!
//! | Module          | Responsibility                                           |
//! |-----------------|----------------------------------------------------------|
//! | [`app`]         | Top-level state machine, action dispatch, key/mouse IO   |
//! | [`github`]      | GraphQL client, auth, inbox + detail fetching, caching   |
//! | [`ui`]          | Every ratatui widget and layout                          |
//! | [`theme`]       | Selectable color palettes                                |
//! | [`config`]      | On-disk user settings (XDG-compliant)                    |
//! | [`state`]       | Persistent session state (active tab, sidebar width, …)  |
//! | [`event`]       | Tokio-bridged crossterm event channel                    |
//! | [`git`]         | Local `git` subprocess wrappers                          |
//! | [`actions_util`]| One-shot OS actions (open browser, copy to clipboard)    |
//! | [`cast`]        | Numeric narrowing helpers with explicit rounding         |
//!
//! ## Authentication
//!
//! The binary resolves a GitHub token in this order:
//! 1. `GITHUB_TOKEN` environment variable.
//! 2. `gh auth token` subprocess (requires the [GitHub CLI] installed and
//!    authenticated).
//!
//! If neither produces a token, the binary exits with a message pointing the
//! user at `gh auth login`. Tokens are never written to disk or into logs.
//!
//! [GitHub CLI]: https://cli.github.com/

#![warn(missing_docs)]

mod actions_util;
mod app;
mod cast;
mod config;
mod event;
mod git;
mod github;
mod state;
mod theme;
mod ui;

use anyhow::{Context, Result};
use app::App;
use clap::{Parser, Subcommand};
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

/// What to dump in `debug dump`.
///
/// Without arguments, prints the inbox. With `pr OWNER/NAME NUMBER` or
/// `issue OWNER/NAME NUMBER`, fetches and prints the respective detail object.
#[derive(Debug, clap::Args)]
struct DumpArgs {
    /// Item kind to fetch detail for: `pr` or `issue`.
    /// Omit entirely to dump the full inbox.
    kind: Option<String>,
    /// Repository slug in `owner/name` form (required when `kind` is set).
    repo: Option<String>,
    /// Item number within the repository (required when `kind` is set).
    number: Option<u32>,
}

/// Debug subcommands for development and troubleshooting.
#[derive(Subcommand, Debug)]
enum DebugCommand {
    /// Fetch data and print raw JSON to stdout, then exit.
    ///
    /// Without extra args: prints the inbox.
    ///
    /// `debug dump pr OWNER/NAME NUMBER` — prints `PrDetail` JSON.
    ///
    /// `debug dump issue OWNER/NAME NUMBER` — prints `IssueDetail` JSON.
    Dump(DumpArgs),
}

/// Top-level developer-facing subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Low-level debugging utilities.
    Debug {
        #[command(subcommand)]
        cmd: DebugCommand,
    },
}

/// octopeek — a keyboard-driven TUI for your GitHub PR and issue inbox.
#[derive(Parser, Debug)]
#[command(
    name = "octopeek",
    about = "A fast, keyboard-driven TUI for your GitHub PR and issue inbox.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments.
    let cli = Cli::parse();

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

    // Handle the `debug dump` subcommand before entering the TUI.
    if let Some(Commands::Debug { cmd: DebugCommand::Dump(args) }) = cli.command {
        return run_debug_dump(args).await;
    }

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

/// Fetch and print data as pretty JSON, then exit without launching the TUI.
///
/// Dispatches to the inbox fetch, PR detail fetch, or issue detail fetch
/// based on the positional `DumpArgs`.
async fn run_debug_dump(args: DumpArgs) -> Result<()> {
    let token = github::auth::load_token()?;
    let client = github::Client::new(token)?;

    match args.kind.as_deref() {
        None => {
            // No kind specified — dump the full inbox.
            let inbox = client.fetch_inbox().await?;
            println!("{}", serde_json::to_string_pretty(&inbox)?);
        }
        Some("pr") => {
            let repo = args.repo.context("`repo` argument is required for `debug dump pr`")?;
            let number =
                args.number.context("`number` argument is required for `debug dump pr`")?;
            let detail = client.fetch_pr_detail(&repo, number).await?;
            println!("{}", serde_json::to_string_pretty(&detail)?);
        }
        Some("issue") => {
            let repo = args.repo.context("`repo` argument is required for `debug dump issue`")?;
            let number =
                args.number.context("`number` argument is required for `debug dump issue`")?;
            let detail = client.fetch_issue_detail(&repo, number).await?;
            println!("{}", serde_json::to_string_pretty(&detail)?);
        }
        Some(other) => {
            anyhow::bail!(
                "unknown dump kind `{other}`: expected `pr` or `issue`, or omit for inbox"
            );
        }
    }

    Ok(())
}
