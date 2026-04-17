//! Application state and main event loop.
//!
//! [`App`] owns all runtime state. [`App::run`] is the async entry point that
//! draws frames and processes actions until the user quits.

pub mod actions;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::config::Config;
use crate::event::EventHandler;
use crate::state::AppSession;
use crate::theme::Palette;
use crate::ui::tabs::Tabs;

use actions::Action;

/// Which high-level panel currently owns keyboard focus.
// `Detail` and `RepoPicker` variants are constructed in Phase 3.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    /// The main dashboard list (PRs or issues).
    #[default]
    Dashboard,
    /// The detail view for a single PR or issue.
    Detail,
    /// The repo-picker overlay.
    RepoPicker,
    /// The full-screen help overlay.
    Help,
}

/// Top-level application state.
pub struct App {
    /// User configuration loaded from disk at startup.
    pub config: Config,
    /// Session state persisted across launches.
    pub session: AppSession,
    /// Active color palette derived from `config.theme`.
    pub palette: Palette,
    /// Open repository tabs.
    pub tabs: Tabs,
    /// `true` until the user quits.
    pub running: bool,
    /// Which widget currently owns keyboard focus.
    pub focus: Focus,
    /// Sender half of the action channel; injected once the event loop starts.
    // Used by background tasks (e.g. auto-refresh) to inject actions.
    pub action_tx: Option<UnboundedSender<Action>>,
    /// Whether the help overlay is currently visible.
    pub show_help: bool,
    /// Handle for the auto-refresh background task, kept alive as long as the
    /// feature is enabled. Dropping this handle cancels the task.
    refresh_handle: Option<JoinHandle<()>>,
}

impl App {
    /// Construct `App` from loaded config and session.
    pub fn new(config: Config, session: AppSession) -> Self {
        // Derive the palette directly from the `Theme` enum — no string
        // parsing needed since `Config` already deserialises to typed enums.
        let palette = Palette::from_theme(config.theme);

        let mut tabs = Tabs::new();
        // Open a tab for every configured repo so the bar is populated
        // immediately on launch, even before any data is fetched.
        for repo in &config.repos {
            tabs.open_or_focus(repo);
        }
        // Restore the active tab index from the previous session.
        tabs.set_active_by_index(session.active_tab_index);

        Self {
            config,
            session,
            palette,
            tabs,
            running: true,
            focus: Focus::Dashboard,
            action_tx: None,
            show_help: false,
            refresh_handle: None,
        }
    }

    /// Run the application: spawn the event thread, start the auto-refresh
    /// timer if configured, then loop drawing frames and processing actions
    /// until `running` is false.
    pub async fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        let (mut events, tx) = EventHandler::new();
        self.action_tx = Some(tx.clone());

        // If auto-refresh is configured, spawn a background task that emits
        // `Action::RefreshAll` on the given cadence. The handle is stored on
        // `App` so it is cancelled automatically when `App` is dropped.
        if let Some(secs) = self.config.auto_refresh_seconds {
            let refresh_tx = tx.clone();
            let interval = tokio::time::Duration::from_secs(u64::from(secs));
            self.refresh_handle = Some(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(interval);
                // Skip the first (immediate) tick so we don't refresh before
                // any data has been loaded.
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    if refresh_tx.send(Action::RefreshAll).is_err() {
                        // Receiver dropped — app is shutting down.
                        break;
                    }
                }
            }));
        }

        while self.running {
            // Draw the current frame.
            terminal.draw(|f| crate::ui::draw(f, &self))?;

            // Wait for the next action from either the input thread or a
            // background task (e.g. the auto-refresh timer).
            let Some(action) = events.next().await else {
                // All senders dropped — shut down.
                break;
            };

            self.handle_action(action);
        }

        // Persist the session so the active tab index survives a relaunch.
        self.session.active_tab_index = self.tabs.active_index().unwrap_or(0);
        self.session.save();

        Ok(())
    }

    /// Route an action to the appropriate handler.
    // Taking `Action` by value is correct — the dispatcher owns and consumes
    // the action. clippy prefers `&Action` here but that would require cloning
    // for variants like `RawKey(KeyEvent)` which are not `Copy`.
    #[allow(clippy::needless_pass_by_value)]
    fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.running = false;
            }
            Action::RawKey(key) => {
                self.handle_key(key);
            }
            Action::Resize(w, h) => {
                // ratatui redraws the full frame on the next loop iteration;
                // no explicit handling is required for resize events.
                debug!("terminal resized to {w}x{h}");
            }
            Action::Mouse(_) => {
                // Mouse support is planned for Phase 3.
                debug!("mouse event received (not yet handled)");
            }
            Action::NextTab => {
                self.tabs.next();
            }
            Action::PrevTab => {
                self.tabs.prev();
            }
            Action::SwitchTab(idx) => {
                self.tabs.set_active_by_index(idx);
            }
            Action::OpenHelp => {
                self.show_help = !self.show_help;
                if self.show_help {
                    self.focus = Focus::Help;
                } else {
                    self.focus = Focus::Dashboard;
                }
            }
            Action::Refresh => {
                warn!("Action::Refresh not yet implemented (Phase 2)");
            }
            Action::RefreshAll => {
                warn!("Action::RefreshAll not yet implemented (Phase 2)");
            }
            Action::OpenDetail => {
                warn!("Action::OpenDetail not yet implemented (Phase 3)");
            }
            Action::BackToDashboard => {
                warn!("Action::BackToDashboard not yet implemented (Phase 3)");
            }
            Action::ToggleView => {
                warn!("Action::ToggleView not yet implemented (Phase 3)");
            }
            Action::OpenInBrowser => {
                warn!("Action::OpenInBrowser not yet implemented (Phase 4)");
            }
            Action::CopyUrl => {
                warn!("Action::CopyUrl not yet implemented (Phase 4)");
            }
            Action::CheckoutBranch => {
                warn!("Action::CheckoutBranch not yet implemented (Phase 5)");
            }
            Action::ConfirmCheckout(_) => {
                warn!("Action::ConfirmCheckout not yet implemented (Phase 5)");
            }
            Action::OpenRepoPicker => {
                warn!("Action::OpenRepoPicker not yet implemented (Phase 3)");
            }
        }
    }

    /// Translate a raw key event into an [`Action`] based on current focus.
    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        // Global bindings that work in any focus state.
        if key.modifiers == KeyModifiers::NONE {
            match key.code {
                KeyCode::Char('q') => {
                    self.running = false;
                    return;
                }
                KeyCode::Char('?') => {
                    self.handle_action(Action::OpenHelp);
                    return;
                }
                KeyCode::Tab => {
                    self.handle_action(Action::NextTab);
                    return;
                }
                _ => {}
            }
        }

        if key.modifiers == KeyModifiers::SHIFT && key.code == KeyCode::BackTab {
            self.handle_action(Action::PrevTab);
            return;
        }

        // Digit keys 1–9 jump to the corresponding tab (1-based).
        if key.modifiers == KeyModifiers::NONE
            && let KeyCode::Char(ch) = key.code
            && let Some(digit) = ch.to_digit(10)
        {
            // digit==0 maps to tab index 9 (vim convention: 0 = 10th tab).
            let idx = if digit == 0 { 9 } else { (digit as usize) - 1 };
            self.tabs.set_active_by_index(idx);
            return;
        }

        // Help overlay steals all other keys — Esc, '?', or 'q' closes it.
        if self.focus == Focus::Help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?' | 'q') => {
                    self.show_help = false;
                    self.focus = Focus::Dashboard;
                }
                _ => {}
            }
            return;
        }

        // Per-focus dispatch.
        match self.focus {
            Focus::Dashboard => self.handle_key_dashboard(key),
            Focus::Detail => {
                warn!("Detail key handling not yet implemented (Phase 3)");
            }
            Focus::RepoPicker => {
                warn!("RepoPicker key handling not yet implemented (Phase 3)");
            }
            Focus::Help => {
                // Handled above; unreachable here.
            }
        }
    }

    /// Key handler for the dashboard (PR/issue list) focus.
    // `self` is required here so this becomes a method once Phase 3 adds
    // real state mutations (cursor movement, selection, etc.).
    #[allow(clippy::unused_self)]
    fn handle_key_dashboard(&self, key: crossterm::event::KeyEvent) {
        if key.modifiers != KeyModifiers::NONE {
            return;
        }
        match key.code {
            // These keys are listed in the help overlay but their actions are
            // not yet implemented; they log at warn level so the user gets
            // immediate feedback that the key was recognised.
            KeyCode::Char('r') => warn!("r: refresh not yet implemented (Phase 2)"),
            KeyCode::Char('R') => warn!("R: refresh-all not yet implemented (Phase 2)"),
            KeyCode::Enter => warn!("Enter: open detail not yet implemented (Phase 3)"),
            KeyCode::Esc => warn!("Esc: back-to-dashboard not yet implemented (Phase 3)"),
            KeyCode::Char('o') => warn!("o: open in browser not yet implemented (Phase 4)"),
            KeyCode::Char('y') => warn!("y: copy URL not yet implemented (Phase 4)"),
            KeyCode::Char('c') => warn!("c: checkout branch not yet implemented (Phase 5)"),
            KeyCode::Char('p') => warn!("p: open repo picker not yet implemented (Phase 3)"),
            KeyCode::Char('i') => warn!("i: toggle view not yet implemented (Phase 3)"),
            KeyCode::Char('j') | KeyCode::Down => {
                warn!("j/Down: list navigation not yet implemented (Phase 3)");
            }
            KeyCode::Char('k') | KeyCode::Up => {
                warn!("k/Up: list navigation not yet implemented (Phase 3)");
            }
            KeyCode::Char('g') => warn!("g: jump to top not yet implemented (Phase 3)"),
            KeyCode::Char('G') => warn!("G: jump to bottom not yet implemented (Phase 3)"),
            KeyCode::Char('n') => warn!("n: next match not yet implemented (Phase 3)"),
            KeyCode::Char('N') => warn!("N: prev match not yet implemented (Phase 3)"),
            KeyCode::Char('f') => warn!("f: filter not yet implemented (Phase 3)"),
            KeyCode::Char('b') => warn!("b: back not yet implemented (Phase 3)"),
            _ => {}
        }
    }
}
