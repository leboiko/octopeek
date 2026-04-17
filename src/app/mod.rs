//! Application state and main event loop.
//!
//! [`App`] owns all runtime state. [`App::run`] is the async entry point that
//! draws frames and processes actions until the user quits.

pub mod actions;

use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::event::EventHandler;
use crate::github;
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
#[allow(clippy::struct_excessive_bools)]
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

    // ── GitHub data ───────────────────────────────────────────────────────────
    /// Authenticated GitHub client, absent if token discovery failed at startup.
    /// Wrapped in `Arc` so it can be shared with background fetch tasks without
    /// requiring `Client: Clone`.
    pub client: Option<Arc<github::Client>>,
    /// Most-recently-fetched inbox, absent until the first successful fetch.
    pub inbox: Option<github::Inbox>,
    /// Human-readable description of the last fetch error, if any.
    pub last_fetch_error: Option<String>,
    /// `true` while a background fetch is in-flight; prevents overlapping fetches.
    pub fetching: bool,
    /// Per-repo selected list index. Key = repo slug, value = 0-based row index.
    pub selection: HashMap<String, usize>,
    /// When the inbox was last successfully loaded (used to display "last synced" text).
    pub inbox_loaded_at: Option<DateTime<Utc>>,
    /// `true` when the user pressed `g` and is waiting for a second `g` (vim-style gg).
    pub pending_g: bool,
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

        // Attempt token discovery at startup; failure is non-fatal — the UI
        // will surface `last_fetch_error` in Phase 3.
        let (client, last_fetch_error) = match github::auth::load_token() {
            Ok(token) => match github::Client::new(token) {
                Ok(c) => (Some(Arc::new(c)), None),
                Err(e) => {
                    tracing::error!("failed to build GitHub client: {e}");
                    (None, Some(e.to_string()))
                }
            },
            Err(e) => {
                tracing::warn!("GitHub token not found: {e}");
                (None, Some(e.to_string()))
            }
        };

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
            client,
            inbox: None,
            last_fetch_error,
            fetching: false,
            selection: HashMap::new(),
            inbox_loaded_at: None,
            pending_g: false,
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

        // Kick off an initial inbox fetch if a client is available and we have
        // no cached data yet.
        if self.client.is_some() && self.inbox.is_none() {
            self.spawn_fetch(tx.clone());
        }

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

        // Abort the auto-refresh task explicitly: `JoinHandle::drop` only
        // detaches in tokio, it does not cancel.
        if let Some(handle) = self.refresh_handle.take() {
            handle.abort();
        }

        // Persist the session so the active tab index survives a relaunch.
        self.session.active_tab_index = self.tabs.active_index().unwrap_or(0);
        self.session.save();

        Ok(())
    }

    /// Spawn a background task that fetches the inbox and sends the result back
    /// via the action channel.  Guards against concurrent fetches via `fetching`.
    fn spawn_fetch(&mut self, tx: tokio::sync::mpsc::UnboundedSender<Action>) {
        if self.fetching {
            debug!("fetch already in progress; skipping");
            return;
        }
        let Some(client) = self.client.clone() else {
            debug!("no GitHub client; skipping fetch");
            return;
        };

        self.fetching = true;
        let _ = tx.send(Action::InboxFetchStarted);

        // Spawn a supervisor task that awaits an inner task's JoinHandle. If
        // the inner task panics, `JoinHandle::await` returns an `Err(JoinError)`
        // with `is_panic() == true`, so we can always emit a terminal action —
        // neither `InboxLoaded` nor `FetchFailed` must ever be skipped, or the
        // `fetching` guard would pin itself on `true` forever.
        tokio::spawn(async move {
            let inner = tokio::spawn(async move { client.fetch_inbox().await });
            let action = match inner.await {
                Ok(Ok(inbox)) => Action::InboxLoaded(Box::new(inbox)),
                Ok(Err(e)) => Action::FetchFailed(e.to_string()),
                Err(join_err) if join_err.is_panic() => {
                    Action::FetchFailed(format!("fetch task panicked: {join_err}"))
                }
                Err(join_err) => Action::FetchFailed(format!("fetch task aborted: {join_err}")),
            };
            let _ = tx.send(action);
        });
    }

    /// Handle a successfully fetched inbox: store data, update tab badges, clear error state.
    fn on_inbox_loaded(&mut self, inbox: github::Inbox) {
        let viewer_login = inbox.viewer_login.clone();

        // Update each tab's needs_action_count from the new inbox. Every open
        // issue assigned to the viewer is counted (the assignment query
        // `assignee:@me` already limits the set to items the viewer is
        // responsible for), while PRs are filtered by primary_flag to exclude
        // Clean and Draft states.
        for tab in &mut self.tabs.tabs {
            let count = inbox
                .prs
                .iter()
                .filter(|pr| pr.repo == tab.repo)
                .filter(|pr| {
                    let flag = pr.primary_flag(&viewer_login);
                    flag != crate::github::flags::ActionFlag::Clean
                        && flag != crate::github::flags::ActionFlag::Draft
                })
                .count()
                + inbox.issues.iter().filter(|i| i.repo == tab.repo).count();
            tab.needs_action_count = Some(count);
        }

        // Clamp any stale per-repo selection indices so they cannot point past
        // the end of the refreshed list (a blocking render bug if a repo
        // shrinks between refreshes). `draw_*_list` clamps defensively too,
        // but keeping the canonical state consistent avoids subtle surprises
        // elsewhere (e.g. the Phase 4 detail view will key off this index).
        for (repo, idx) in &mut self.selection {
            let max_pr = inbox.prs.iter().filter(|pr| pr.repo == *repo).count();
            let max_issue = inbox.issues.iter().filter(|i| i.repo == *repo).count();
            let max = max_pr.max(max_issue);
            if max == 0 {
                *idx = 0;
            } else if *idx >= max {
                *idx = max - 1;
            }
        }

        self.inbox = Some(inbox);
        self.inbox_loaded_at = Some(Utc::now());
        self.fetching = false;
        self.last_fetch_error = None;
    }

    /// Handle a failed fetch: record the error, keep any cached inbox.
    fn on_fetch_failed(&mut self, err: String) {
        self.fetching = false;
        warn!("GitHub inbox fetch failed: {err}");
        self.last_fetch_error = Some(err);
    }

    /// Return the filtered + sorted PR list length for the active repo.
    fn active_list_len(&self) -> usize {
        let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) else {
            return 0;
        };
        let Some(inbox) = &self.inbox else {
            return 0;
        };
        let mode = self.session.view_mode(&repo);
        match mode {
            crate::state::ViewMode::Prs => inbox.prs.iter().filter(|p| p.repo == repo).count(),
            crate::state::ViewMode::Issues => {
                inbox.issues.iter().filter(|i| i.repo == repo).count()
            }
        }
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
            Action::Refresh | Action::RefreshAll => {
                if let Some(tx) = self.action_tx.clone() {
                    self.spawn_fetch(tx);
                }
            }
            Action::OpenDetail => {
                warn!("Action::OpenDetail not yet implemented (Phase 3)");
            }
            Action::BackToDashboard => {
                warn!("Action::BackToDashboard not yet implemented (Phase 3)");
            }
            Action::ToggleView => {
                if let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) {
                    let current = self.session.view_mode(&repo);
                    let next = match current {
                        crate::state::ViewMode::Prs => crate::state::ViewMode::Issues,
                        crate::state::ViewMode::Issues => crate::state::ViewMode::Prs,
                    };
                    self.session.set_view_mode(&repo, next);
                    // Reset selection to 0 when toggling view.
                    self.selection.insert(repo, 0);
                }
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
            Action::InboxFetchStarted => {
                self.fetching = true;
            }
            Action::InboxLoaded(inbox) => {
                self.on_inbox_loaded(*inbox);
            }
            Action::FetchFailed(msg) => {
                self.on_fetch_failed(msg);
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
    fn handle_key_dashboard(&mut self, key: crossterm::event::KeyEvent) {
        if key.modifiers != KeyModifiers::NONE {
            self.pending_g = false;
            return;
        }

        // Resolve active repo slug once.
        let active_repo = self.tabs.active_tab().map(|t| t.repo.clone());

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.pending_g = false;
                if let Some(repo) = active_repo {
                    let len = self.active_list_len();
                    if len > 0 {
                        let sel = self.selection.entry(repo).or_insert(0);
                        *sel = (*sel + 1).min(len - 1);
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.pending_g = false;
                if let Some(repo) = active_repo {
                    let sel = self.selection.entry(repo).or_insert(0);
                    *sel = sel.saturating_sub(1);
                }
            }
            KeyCode::Char('g') => {
                if self.pending_g {
                    // Second `g` — jump to top.
                    self.pending_g = false;
                    if let Some(repo) = active_repo {
                        self.selection.insert(repo, 0);
                    }
                } else {
                    self.pending_g = true;
                }
            }
            KeyCode::Char('G') => {
                self.pending_g = false;
                if let Some(repo) = active_repo {
                    let len = self.active_list_len();
                    let bottom = if len > 0 { len - 1 } else { 0 };
                    self.selection.insert(repo, bottom);
                }
            }
            KeyCode::Char('i') => {
                self.pending_g = false;
                self.handle_action(Action::ToggleView);
            }
            KeyCode::Char('r') => {
                self.pending_g = false;
                self.handle_action(Action::Refresh);
            }
            KeyCode::Char('R') => {
                self.pending_g = false;
                self.handle_action(Action::RefreshAll);
            }
            KeyCode::Enter => {
                self.pending_g = false;
                info!("Phase 4: not yet implemented — open detail");
            }
            KeyCode::Char('o') => {
                self.pending_g = false;
                info!("Phase 4: not yet implemented — open in browser");
            }
            KeyCode::Char('y') => {
                self.pending_g = false;
                info!("Phase 4: not yet implemented — copy URL");
            }
            KeyCode::Char('c') => {
                self.pending_g = false;
                info!("Phase 5: not yet implemented — checkout branch");
            }
            KeyCode::Char('p') => {
                self.pending_g = false;
                info!("Phase 5: not yet implemented — repo picker");
            }
            KeyCode::Char('f') => {
                self.pending_g = false;
                info!("Phase 4: not yet implemented — filter");
            }
            KeyCode::Char('n') => {
                self.pending_g = false;
                info!("Phase 4: not yet implemented — next match");
            }
            KeyCode::Char('N') => {
                self.pending_g = false;
                info!("Phase 4: not yet implemented — prev match");
            }
            KeyCode::Char('b') => {
                self.pending_g = false;
                info!("Phase 4: not yet implemented — back");
            }
            // All other keys (including Esc) cancel any pending chord.
            _ => {
                self.pending_g = false;
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::github::types::{
        CheckState, Inbox, Issue, Label, MergeStateStatus, Mergeable, PullRequest, Review,
        ReviewDecision, Role,
    };

    /// Build a minimal clean PR for use in tests.
    fn make_pr(repo: &str, flag_variant: &str, viewer: &str) -> PullRequest {
        let mut pr = PullRequest {
            number: 1,
            title: "Test PR".to_owned(),
            url: "https://github.com/o/r/pull/1".to_owned(),
            repo: repo.to_owned(),
            author: viewer.to_owned(),
            is_draft: false,
            mergeable: Mergeable::Mergeable,
            merge_state: MergeStateStatus::Clean,
            review_decision: None,
            commits_count: 1,
            comments_count: 0,
            check_state: Some(CheckState::Success),
            failing_checks: vec![],
            unresolved_threads: 0,
            requested_reviewers: vec![],
            reviews: vec![],
            updated_at: Utc::now(),
            roles: vec![Role::Author],
        };
        match flag_variant {
            "conflict" => pr.mergeable = Mergeable::Conflicting,
            "review_requested" => pr.requested_reviewers = vec![viewer.to_owned()],
            "draft" => pr.is_draft = true,
            "changes" => pr.review_decision = Some(ReviewDecision::ChangesRequested),
            _ => {} // clean
        }
        pr
    }

    #[allow(dead_code)]
    fn make_issue(repo: &str) -> Issue {
        Issue {
            number: 1,
            title: "Test Issue".to_owned(),
            url: "https://github.com/o/r/issues/1".to_owned(),
            repo: repo.to_owned(),
            author: "viewer".to_owned(),
            comments_count: 0,
            updated_at: Utc::now(),
            labels: vec![Label { name: "bug".to_owned(), color: "ee0701".to_owned() }],
        }
    }

    /// `on_inbox_loaded` must correctly count needs-action PRs for a tab
    /// (excluding Draft and Clean) and update `tab.needs_action_count`.
    #[test]
    fn on_inbox_loaded_sets_needs_action_count() {
        let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        let inbox = Inbox {
            viewer_login: "viewer".to_owned(),
            prs: vec![
                make_pr("o/r", "conflict", "viewer"),         // needs action
                make_pr("o/r", "review_requested", "viewer"), // needs action
                make_pr("o/r", "draft", "viewer"),            // NOT needs action
                make_pr("o/r", "clean", "viewer"),            // NOT needs action
                make_pr("other/repo", "conflict", "viewer"),  // different repo
            ],
            issues: vec![],
        };

        app.on_inbox_loaded(inbox);

        let tab = app.tabs.tabs.iter().find(|t| t.repo == "o/r").expect("tab for o/r");
        assert_eq!(
            tab.needs_action_count,
            Some(2),
            "Expected 2 action items in o/r, got {:?}",
            tab.needs_action_count
        );
    }

    /// After `on_inbox_loaded`, fetching is false and error is cleared.
    #[test]
    fn on_inbox_loaded_clears_error_and_fetching() {
        let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.fetching = true;
        app.last_fetch_error = Some("prior error".to_owned());

        let inbox = Inbox { viewer_login: "viewer".to_owned(), prs: vec![], issues: vec![] };
        app.on_inbox_loaded(inbox);

        assert!(!app.fetching);
        assert!(app.last_fetch_error.is_none());
        assert!(app.inbox_loaded_at.is_some());
    }

    /// When a refresh shrinks a repo's list, stale selection indices must be
    /// clamped so the dashboard cannot render a cursor past the end of the list.
    #[test]
    fn on_inbox_loaded_clamps_stale_selection() {
        let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        // Simulate: earlier refresh had 5 PRs and the user moved the cursor to row 4.
        app.selection.insert("o/r".to_owned(), 4);

        // Now the refresh returns only 2 PRs in "o/r".
        let inbox = Inbox {
            viewer_login: "viewer".to_owned(),
            prs: vec![make_pr("o/r", "clean", "viewer"), make_pr("o/r", "conflict", "viewer")],
            issues: vec![],
        };
        app.on_inbox_loaded(inbox);

        assert_eq!(app.selection.get("o/r"), Some(&1), "stale index 4 must clamp to len-1 = 1");
    }

    /// When a refresh removes every item for a repo, the stored selection must
    /// collapse to 0 rather than attempting len-1 = `usize::MAX` underflow.
    #[test]
    fn on_inbox_loaded_clamps_empty_list() {
        let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.selection.insert("o/r".to_owned(), 3);

        let inbox = Inbox { viewer_login: "viewer".to_owned(), prs: vec![], issues: vec![] };
        app.on_inbox_loaded(inbox);

        assert_eq!(app.selection.get("o/r"), Some(&0));
    }

    /// `on_fetch_failed` sets the error string and clears `fetching`.
    #[test]
    fn on_fetch_failed_records_error() {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.fetching = true;

        app.on_fetch_failed("network timeout".to_owned());

        assert!(!app.fetching);
        assert_eq!(app.last_fetch_error.as_deref(), Some("network timeout"));
    }

    /// Unused fields added to avoid "unused import" warnings from the test helpers.
    #[allow(dead_code)]
    fn _use_types(_r: Review, _rd: ReviewDecision) {}
}
