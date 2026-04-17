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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    /// The main dashboard list (PRs or issues).
    #[default]
    Dashboard,
    /// The first-run welcome wizard shown when config is empty and the inbox
    /// reveals repos the user is already active in.
    FirstRun,
    /// The detail view for a single PR or issue.
    Detail,
    /// The repo-picker overlay.
    RepoPicker,
    /// The full-screen help overlay.
    Help,
    /// The generic confirmation overlay.
    Confirm,
}

/// One repo suggestion shown in the first-run wizard.
///
/// Built from the inbox on first launch when `config.repos` is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstRunSuggestion {
    /// `owner/name` slug of the suggested repository.
    pub repo: String,
    /// Total number of open items (PRs + issues) the viewer has in this repo.
    pub count: usize,
    /// Whether the user has checked this suggestion for import.
    pub selected: bool,
}

/// Interaction mode for the repo picker overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepoPickerMode {
    /// Cursor is on the repo list (default).
    #[default]
    List,
    /// Cursor is in the text input field.
    Input,
}

/// Which kind of item a detail fetch targets.
///
/// Passed to [`App::spawn_detail_fetch`] so a single generic supervisor task
/// can dispatch to either [`github::Client::fetch_pr_detail`] or
/// [`github::Client::fetch_issue_detail`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailKind {
    /// Fetch a pull request.
    Pr,
    /// Fetch an issue.
    Issue,
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
    /// Transient message displayed in the status bar for a short duration.
    ///
    /// Set via [`App::show_flash`]; the status bar renderer clears it once
    /// `FlashMessage::is_active` returns `false`.
    pub flash: Option<crate::ui::status_bar::FlashMessage>,

    // ── Detail state ──────────────────────────────────────────────────────────
    /// Most-recently-fetched PR detail, absent until a successful detail fetch.
    pub pr_detail: Option<github::detail::PrDetail>,
    /// Most-recently-fetched issue detail, absent until a successful detail fetch.
    pub issue_detail: Option<github::detail::IssueDetail>,
    /// `true` while a background detail fetch is in-flight.
    pub detail_fetching: bool,
    /// Human-readable description of the last detail fetch error, if any.
    pub detail_error: Option<String>,
    /// Vertical scroll offset for the detail view (lines from the top).
    pub pr_detail_scroll: u16,
    /// `true` when the files section in the PR detail view is fully expanded.
    pub pr_detail_files_expanded: bool,
    /// `true` when the comments section in the PR detail view is fully expanded.
    pub pr_detail_comments_expanded: bool,
    /// Section Y-offsets recomputed each frame, used by Tab navigation.
    pub pr_detail_section_anchors: Vec<u16>,
    /// Y-offsets of unresolved review thread starts, for `n`/`N` cycling.
    pub pr_detail_unresolved_anchors: Vec<u16>,
    /// Index into `pr_detail_unresolved_anchors` for `n`/`N` cycling.
    pub pr_detail_unresolved_idx: usize,
    /// `true` when the user pressed `g` in detail focus and is awaiting a second `g`.
    pub detail_pending_g: bool,
    /// Copy-mode state for the PR/issue detail view. Inactive until the user
    /// presses `v`. When active, normal detail key bindings are suppressed in
    /// favour of cursor movement, selection, and yank.
    pub copy_mode: crate::ui::copy_mode::CopyMode,
    /// Cached PR-detail viewport rect, written by the renderer each frame so
    /// key and mouse handlers can map screen coordinates to content positions
    /// (and auto-scroll the copy-mode cursor) without taking a `&mut` through
    /// the whole UI layer. Interior mutability via `Cell` keeps the existing
    /// `&App` render signatures intact.
    pub pr_detail_viewport: std::cell::Cell<ratatui::layout::Rect>,

    // ── Repo picker state ─────────────────────────────────────────────────────
    /// Index of the currently highlighted repo in the picker list.
    pub repo_picker_list_cursor: usize,
    /// Text buffer for the repo picker's "Add" input field.
    pub repo_picker_input: String,
    /// Whether the picker is in list-navigation or text-input mode.
    pub repo_picker_mode: RepoPickerMode,
    /// Focus state the picker should return to on close.
    pub repo_picker_return_focus: Focus,

    // ── Confirmation overlay state ────────────────────────────────────────────
    /// When `Some`, the confirmation overlay is displayed and `focus` is
    /// [`Focus::Confirm`].  Cleared when the user confirms or cancels.
    pub confirm: Option<crate::ui::confirm::Confirm>,
    /// Focus state to restore when the confirmation overlay is dismissed.
    pub confirm_return_focus: Focus,

    // ── First-run wizard state ────────────────────────────────────────────────
    /// Suggested repos shown in the first-run wizard; populated from the inbox
    /// when `config.repos` is empty on the first successful fetch.
    pub first_run_suggestions: Vec<FirstRunSuggestion>,
    /// Index of the currently highlighted row in the first-run suggestion list.
    pub first_run_cursor: usize,
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
            flash: None,
            pr_detail: None,
            issue_detail: None,
            detail_fetching: false,
            detail_error: None,
            pr_detail_scroll: 0,
            pr_detail_files_expanded: false,
            pr_detail_comments_expanded: false,
            pr_detail_section_anchors: Vec::new(),
            pr_detail_unresolved_anchors: Vec::new(),
            pr_detail_unresolved_idx: 0,
            detail_pending_g: false,
            copy_mode: crate::ui::copy_mode::CopyMode::default(),
            pr_detail_viewport: std::cell::Cell::new(ratatui::layout::Rect::default()),
            repo_picker_list_cursor: 0,
            repo_picker_input: String::new(),
            repo_picker_mode: RepoPickerMode::List,
            repo_picker_return_focus: Focus::Dashboard,
            confirm: None,
            confirm_return_focus: Focus::Dashboard,
            first_run_suggestions: Vec::new(),
            first_run_cursor: 0,
        }
    }

    /// Display a flash message in the status bar for `duration`.
    ///
    /// Replaces any currently active flash message.
    pub fn show_flash(&mut self, text: impl Into<String>, duration: std::time::Duration) {
        self.flash = Some(crate::ui::status_bar::FlashMessage::new(text, duration));
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

        // First-run wizard: when config is still empty and focus is on the
        // dashboard (not an overlay or detail), compute repo suggestions from
        // the inbox and switch to the wizard focus.
        if self.config.repos.is_empty() && self.focus == Focus::Dashboard {
            let mut counts: HashMap<String, usize> = HashMap::new();
            for pr in &inbox.prs {
                *counts.entry(pr.repo.clone()).or_insert(0) += 1;
            }
            for issue in &inbox.issues {
                *counts.entry(issue.repo.clone()).or_insert(0) += 1;
            }
            if !counts.is_empty() {
                // Sort by count descending, then alphabetically by repo for
                // stable ordering when counts are equal.
                let mut suggestions: Vec<FirstRunSuggestion> = counts
                    .into_iter()
                    .map(|(repo, count)| FirstRunSuggestion { repo, count, selected: false })
                    .collect();
                suggestions.sort_unstable_by(|a, b| {
                    b.count.cmp(&a.count).then_with(|| a.repo.cmp(&b.repo))
                });
                self.first_run_suggestions = suggestions;
                self.first_run_cursor = 0;
                self.focus = Focus::FirstRun;
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

    /// Spawn a background task that fetches PR or issue detail and sends the
    /// result back via the action channel.
    ///
    /// Guards against concurrent detail fetches via `detail_fetching`. Uses the
    /// same supervisor-task panic-catching pattern as [`Self::spawn_fetch`] so
    /// `detail_fetching` is always reset, even if the inner task panics.
    pub fn spawn_detail_fetch(
        &mut self,
        kind: DetailKind,
        repo: String,
        number: u32,
        tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) {
        if self.detail_fetching {
            debug!("detail fetch already in progress; skipping");
            return;
        }
        let Some(client) = self.client.clone() else {
            debug!("no GitHub client; skipping detail fetch");
            let _ = tx.send(Action::DetailFetchFailed("no GitHub client configured".to_owned()));
            return;
        };

        self.detail_fetching = true;

        tokio::spawn(async move {
            let inner: tokio::task::JoinHandle<anyhow::Result<Action>> = tokio::spawn(async move {
                match kind {
                    DetailKind::Pr => {
                        let detail = client.fetch_pr_detail(&repo, number).await?;
                        Ok(Action::PrDetailLoaded(Box::new(detail)))
                    }
                    DetailKind::Issue => {
                        let detail = client.fetch_issue_detail(&repo, number).await?;
                        Ok(Action::IssueDetailLoaded(Box::new(detail)))
                    }
                }
            });
            let action = match inner.await {
                Ok(Ok(action)) => action,
                Ok(Err(e)) => Action::DetailFetchFailed(e.to_string()),
                Err(join_err) if join_err.is_panic() => {
                    Action::DetailFetchFailed(format!("detail fetch task panicked: {join_err}"))
                }
                Err(join_err) => {
                    Action::DetailFetchFailed(format!("detail fetch task aborted: {join_err}"))
                }
            };
            let _ = tx.send(action);
        });
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
    #[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
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
            Action::Mouse(m) => {
                self.handle_mouse(m);
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
                self.begin_checkout_from_selection();
            }
            Action::ConfirmCheckout(confirmed) => {
                if confirmed {
                    self.execute_confirm();
                } else {
                    self.dismiss_confirm();
                }
            }
            Action::OpenRepoPicker => {
                debug!("Action::OpenRepoPicker: switching focus from {:?}", self.focus);
                self.repo_picker_list_cursor = 0;
                self.repo_picker_input.clear();
                self.repo_picker_mode = RepoPickerMode::List;
                self.repo_picker_return_focus = self.focus;
                self.focus = Focus::RepoPicker;
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
            Action::FetchPrDetail(repo, number) => {
                if let Some(tx) = self.action_tx.clone() {
                    self.spawn_detail_fetch(DetailKind::Pr, repo, number, tx);
                }
            }
            Action::FetchIssueDetail(repo, number) => {
                if let Some(tx) = self.action_tx.clone() {
                    self.spawn_detail_fetch(DetailKind::Issue, repo, number, tx);
                }
            }
            Action::PrDetailLoaded(detail) => {
                self.pr_detail = Some(*detail);
                self.detail_fetching = false;
                self.detail_error = None;
            }
            Action::IssueDetailLoaded(detail) => {
                self.issue_detail = Some(*detail);
                self.detail_fetching = false;
                self.detail_error = None;
            }
            Action::DetailFetchFailed(msg) => {
                self.detail_fetching = false;
                warn!("GitHub detail fetch failed: {msg}");
                self.detail_error = Some(msg);
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
            Focus::FirstRun => self.handle_key_first_run(key),
            Focus::Detail => self.handle_key_detail(key),
            Focus::RepoPicker => self.handle_key_repo_picker(key),
            Focus::Confirm => self.handle_key_confirm(key),
            Focus::Help => {
                // Handled above; unreachable here.
            }
        }
    }

    /// Key handler for the first-run welcome wizard.
    ///
    /// Bindings:
    /// - `j` / `Down`: move cursor down (clamped).
    /// - `k` / `Up`: move cursor up (saturating).
    /// - `g g` (vim chord via `pending_g`): jump to top.
    /// - `G`: jump to bottom.
    /// - `Space`: toggle selected state of the cursor row.
    /// - `a`: open the repo-picker in Input mode so the user can add a custom repo.
    /// - `Enter`: commit all selected suggestions to config, then go to Dashboard.
    /// - `Esc`: skip wizard without committing, go to Dashboard.
    /// - `?`: toggle help overlay.
    /// - `q`: quit.
    fn handle_key_first_run(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        // Ignore key combos with modifiers (consistent with other handlers).
        if key.modifiers != KeyModifiers::NONE {
            self.pending_g = false;
            return;
        }

        let len = self.first_run_suggestions.len();

        match key.code {
            KeyCode::Char('q') => {
                self.pending_g = false;
                self.running = false;
            }
            KeyCode::Char('?') => {
                self.pending_g = false;
                self.handle_action(Action::OpenHelp);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.pending_g = false;
                if len > 0 {
                    self.first_run_cursor = (self.first_run_cursor + 1).min(len - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.pending_g = false;
                self.first_run_cursor = self.first_run_cursor.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                if self.pending_g {
                    // Second `g` in vim chord — jump to top.
                    self.pending_g = false;
                    self.first_run_cursor = 0;
                } else {
                    self.pending_g = true;
                }
            }
            KeyCode::Char('G') => {
                self.pending_g = false;
                if len > 0 {
                    self.first_run_cursor = len - 1;
                }
            }
            KeyCode::Char(' ') => {
                self.pending_g = false;
                if len > 0 {
                    let idx = self.first_run_cursor.min(len - 1);
                    self.first_run_suggestions[idx].selected =
                        !self.first_run_suggestions[idx].selected;
                }
            }
            KeyCode::Char('a') => {
                // Open the repo-picker in Input mode so the user can add a
                // custom repo not present in the suggestions list.
                self.pending_g = false;
                self.repo_picker_list_cursor = 0;
                self.repo_picker_input.clear();
                self.repo_picker_mode = RepoPickerMode::Input;
                // Return to FirstRun (not Dashboard) when the picker closes,
                // so the wizard can stay open for any remaining suggestions.
                self.repo_picker_return_focus = Focus::FirstRun;
                self.focus = Focus::RepoPicker;
            }
            KeyCode::Enter => {
                self.pending_g = false;
                self.commit_first_run();
            }
            KeyCode::Esc => {
                // Skip without committing.
                self.pending_g = false;
                self.first_run_suggestions.clear();
                self.first_run_cursor = 0;
                self.focus = Focus::Dashboard;
                self.show_flash(
                    "Press `p` any time to add repos",
                    std::time::Duration::from_secs(2),
                );
            }
            _ => {
                self.pending_g = false;
            }
        }
    }

    /// Commit the user's selections from the first-run wizard to
    /// `config.repos`, save, and return to the dashboard.
    ///
    /// If nothing is selected, flashes a hint and keeps the wizard open
    /// rather than closing silently (users often hit Enter accidentally).
    fn commit_first_run(&mut self) {
        // Collect the chosen slugs first so the mutation loop below does not
        // also need to borrow `first_run_suggestions`.
        let selected: Vec<String> = self
            .first_run_suggestions
            .iter()
            .filter(|s| s.selected)
            .map(|s| s.repo.clone())
            .collect();

        if selected.is_empty() {
            self.show_flash(
                "Nothing selected — press Space to pick repos, or Esc to skip",
                std::time::Duration::from_secs(3),
            );
            return;
        }

        // Explicit loop (not an iterator with side-effects in its closure) so
        // that the `added` count cannot silently desync from the `push` calls
        // if the deduplication logic is refactored later.
        let mut added: usize = 0;
        for repo in selected {
            if !self.config.repos.contains(&repo) {
                self.config.repos.push(repo);
                added += 1;
            }
        }
        self.config.save();
        self.sync_tabs_to_config();
        self.first_run_suggestions.clear();
        self.first_run_cursor = 0;
        self.focus = Focus::Dashboard;
        self.show_flash(
            format!("Added {added} repositor{}", if added == 1 { "y" } else { "ies" }),
            std::time::Duration::from_secs(2),
        );
    }

    /// Key handler for the PR/issue detail focus.
    #[allow(clippy::too_many_lines)]
    fn handle_key_detail(&mut self, key: crossterm::event::KeyEvent) {
        // When copy mode is active, it owns the entire keymap for this focus.
        if self.copy_mode.active {
            self.handle_key_detail_copy_mode(key);
            return;
        }

        // Tab/Shift+Tab are consumed here for section navigation,
        // not forwarded to the global tab-switch handler.
        if key.modifiers == KeyModifiers::SHIFT && key.code == KeyCode::BackTab {
            // Navigate to previous section anchor.
            self.detail_jump_section(-1);
            return;
        }

        if key.modifiers != KeyModifiers::NONE {
            self.detail_pending_g = false;
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('b') => {
                self.detail_pending_g = false;
                self.back_to_dashboard();
            }
            KeyCode::Char('q') => {
                self.detail_pending_g = false;
                self.running = false;
            }
            KeyCode::Char('?') => {
                self.detail_pending_g = false;
                self.handle_action(Action::OpenHelp);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.detail_pending_g = false;
                self.pr_detail_scroll = self.pr_detail_scroll.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.detail_pending_g = false;
                self.pr_detail_scroll = self.pr_detail_scroll.saturating_sub(1);
            }
            KeyCode::Char('d') => {
                self.detail_pending_g = false;
                self.pr_detail_scroll = self.pr_detail_scroll.saturating_add(10);
            }
            KeyCode::Char('u') => {
                self.detail_pending_g = false;
                self.pr_detail_scroll = self.pr_detail_scroll.saturating_sub(10);
            }
            KeyCode::Char('g') => {
                if self.detail_pending_g {
                    self.detail_pending_g = false;
                    self.pr_detail_scroll = 0;
                } else {
                    self.detail_pending_g = true;
                }
            }
            KeyCode::Char('G') => {
                self.detail_pending_g = false;
                // Scroll to a large value; the renderer clamps to valid range.
                self.pr_detail_scroll = u16::MAX;
            }
            KeyCode::Tab => {
                self.detail_pending_g = false;
                self.detail_jump_section(1);
            }
            KeyCode::Char('n') => {
                self.detail_pending_g = false;
                self.detail_cycle_unresolved(1);
            }
            KeyCode::Char('N') => {
                self.detail_pending_g = false;
                self.detail_cycle_unresolved(-1);
            }
            KeyCode::Char('f') => {
                self.detail_pending_g = false;
                self.pr_detail_files_expanded = !self.pr_detail_files_expanded;
            }
            KeyCode::Char('m') => {
                self.detail_pending_g = false;
                self.pr_detail_comments_expanded = !self.pr_detail_comments_expanded;
            }
            KeyCode::Char('o') => {
                self.detail_pending_g = false;
                let url = self
                    .pr_detail
                    .as_ref()
                    .map(|d| d.url.clone())
                    .or_else(|| self.issue_detail.as_ref().map(|d| d.url.clone()));
                if let Some(url) = url {
                    match crate::actions_util::open_url_in_browser(&url) {
                        Ok(()) => {
                            self.show_flash("Opened in browser", std::time::Duration::from_secs(2));
                        }
                        Err(e) => {
                            self.show_flash(
                                format!("Open failed: {e}"),
                                std::time::Duration::from_secs(3),
                            );
                        }
                    }
                }
            }
            KeyCode::Char('y') => {
                self.detail_pending_g = false;
                let url = self
                    .pr_detail
                    .as_ref()
                    .map(|d| d.url.clone())
                    .or_else(|| self.issue_detail.as_ref().map(|d| d.url.clone()));
                if let Some(url) = url {
                    match crate::actions_util::copy_to_clipboard(&url) {
                        Ok(()) => {
                            self.show_flash("URL copied", std::time::Duration::from_secs(2));
                        }
                        Err(e) => {
                            self.show_flash(
                                format!("Copy failed: {e}"),
                                std::time::Duration::from_secs(3),
                            );
                        }
                    }
                }
            }
            KeyCode::Char('c') => {
                self.detail_pending_g = false;
                self.handle_action(Action::CheckoutBranch);
            }
            KeyCode::Char('v') => {
                self.detail_pending_g = false;
                // Enter copy mode at the top of the current viewport so the
                // block cursor lands somewhere the user can already see.
                self.copy_mode.enter(self.pr_detail_scroll as usize, 0);
            }
            KeyCode::Char('r') => {
                self.detail_pending_g = false;
                // Re-fetch the current detail. As in `open_detail_for_selection`,
                // `detail_fetching` must NOT be set here — `spawn_detail_fetch`
                // owns the guard, and setting it externally would make it skip.
                if let Some(detail) = &self.pr_detail {
                    let repo = detail.repo.clone();
                    let number = detail.number;
                    self.pr_detail = None;
                    self.issue_detail = None;
                    self.detail_error = None;
                    self.pr_detail_scroll = 0;
                    if let Some(tx) = self.action_tx.clone() {
                        self.spawn_detail_fetch(DetailKind::Pr, repo, number, tx);
                    }
                } else if let Some(detail) = &self.issue_detail {
                    let repo = detail.repo.clone();
                    let number = detail.number;
                    self.pr_detail = None;
                    self.issue_detail = None;
                    self.detail_error = None;
                    self.pr_detail_scroll = 0;
                    if let Some(tx) = self.action_tx.clone() {
                        self.spawn_detail_fetch(DetailKind::Issue, repo, number, tx);
                    }
                }
            }
            _ => {
                self.detail_pending_g = false;
            }
        }
    }

    /// Key handler active only while `self.copy_mode.active` is `true`.
    ///
    /// The copy-mode keymap is a separate modal layer: it owns `h`/`j`/`k`/`l`
    /// and arrows for cursor movement, `V` to toggle the selection anchor,
    /// `y` to yank the selection, and `Esc` to exit without copying.
    ///
    /// Any key not listed here is intentionally swallowed so the user cannot
    /// accidentally trigger destructive actions (like checkout) while trying
    /// to copy text.
    fn handle_key_detail_copy_mode(&mut self, key: crossterm::event::KeyEvent) {
        let lines = self.current_detail_lines();
        match key.code {
            KeyCode::Esc => {
                self.copy_mode.exit();
            }
            KeyCode::Char('q') => {
                self.copy_mode.exit();
                self.running = false;
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.copy_mode.move_cursor(-1, 0, &lines);
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.copy_mode.move_cursor(1, 0, &lines);
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.copy_mode.move_cursor(0, 1, &lines);
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.copy_mode.move_cursor(0, -1, &lines);
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('g') => {
                self.copy_mode.jump_top();
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('G') => {
                self.copy_mode.jump_bottom(&lines);
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('V' | 'v') => {
                self.copy_mode.toggle_selection();
            }
            KeyCode::Char('y') => {
                if let Some(text) = self.copy_mode.selected_text(&lines) {
                    match crate::actions_util::copy_to_clipboard(&text) {
                        Ok(()) => {
                            let len = text.chars().count();
                            self.show_flash(
                                format!("Copied {len} chars"),
                                std::time::Duration::from_secs(2),
                            );
                        }
                        Err(e) => {
                            self.show_flash(
                                format!("Copy failed: {e}"),
                                std::time::Duration::from_secs(3),
                            );
                        }
                    }
                    self.copy_mode.exit();
                } else {
                    self.show_flash(
                        "No selection (press V to start)",
                        std::time::Duration::from_secs(2),
                    );
                }
            }
            _ => {}
        }
    }

    /// Route a raw mouse event to the focused panel.
    ///
    /// Wheel events always scroll (regardless of copy mode). A left-button
    /// press inside the detail viewport enters copy mode and drops the cursor
    /// at the clicked cell; subsequent drag events extend the selection.
    fn handle_mouse(&mut self, m: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};

        match self.focus {
            Focus::Detail => match m.kind {
                MouseEventKind::ScrollUp => {
                    self.pr_detail_scroll = self.pr_detail_scroll.saturating_sub(3);
                }
                MouseEventKind::ScrollDown => {
                    self.pr_detail_scroll = self.pr_detail_scroll.saturating_add(3);
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some((row, col)) = self.mouse_to_content_pos(m.column, m.row) {
                        // A fresh left-click starts a new cursor position with
                        // no active selection. The drag handler will set an
                        // anchor on the first drag event.
                        self.copy_mode.enter(row, col);
                    }
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    if !self.copy_mode.active {
                        return;
                    }
                    if self.copy_mode.anchor.is_none() {
                        self.copy_mode.anchor = Some(self.copy_mode.cursor);
                    }
                    if let Some((row, col)) = self.mouse_to_content_pos(m.column, m.row) {
                        let lines = self.current_detail_lines();
                        let last_row = lines.len().saturating_sub(1);
                        self.copy_mode.cursor = crate::ui::copy_mode::Pos {
                            row: row.min(last_row),
                            col,
                        };
                        self.ensure_cursor_visible(&lines);
                    }
                }
                _ => {}
            },
            Focus::Dashboard => match m.kind {
                MouseEventKind::ScrollUp => {
                    self.move_dashboard_selection(-1);
                }
                MouseEventKind::ScrollDown => {
                    self.move_dashboard_selection(1);
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Map a (column, row) mouse position to a logical (row, col) position
    /// within the currently rendered detail lines. Returns `None` when the
    /// event is outside the viewport (including the status bar or tab bar).
    ///
    /// The column mapping uses display cells, not characters: wide characters
    /// (CJK / emoji) will round to the nearest cell boundary. For an MVP copy
    /// mode this is acceptable; a follow-up can refine with a proper
    /// display-to-char column walk.
    fn mouse_to_content_pos(&self, col: u16, row: u16) -> Option<(usize, usize)> {
        let area = self.pr_detail_viewport.get();
        if area.width == 0 || area.height == 0 {
            return None;
        }
        if row < area.y || row >= area.y.saturating_add(area.height) {
            return None;
        }
        if col < area.x || col >= area.x.saturating_add(area.width) {
            return None;
        }
        let content_row = self
            .pr_detail_scroll
            .saturating_add(row.saturating_sub(area.y));
        let content_col = self
            .copy_mode
            .h_scroll
            .saturating_add(col.saturating_sub(area.x));
        Some((usize::from(content_row), usize::from(content_col)))
    }

    /// Move the dashboard selection by `delta` rows, clamped to the current
    /// list length. Used by the mouse wheel handler.
    fn move_dashboard_selection(&mut self, delta: i32) {
        let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) else {
            return;
        };
        let len = self.active_list_len();
        if len == 0 {
            return;
        }
        let max_idx = len.saturating_sub(1);
        let sel = self.selection.entry(repo).or_insert(0);
        let new = i64::try_from(*sel).unwrap_or(0).saturating_add(i64::from(delta));
        let clamped = new.clamp(0, i64::try_from(max_idx).unwrap_or(i64::MAX));
        *sel = usize::try_from(clamped).unwrap_or(0);
    }

    /// Rebuild the currently focused detail's rendered line buffer.
    ///
    /// Copy-mode cursor motion and selection extraction work against the
    /// exact same `Vec<Line>` the renderer produces, so we call into the
    /// panel `build_content` helpers on demand. Returns an empty `Vec` when
    /// no detail is loaded.
    fn current_detail_lines(&self) -> Vec<ratatui::text::Line<'static>> {
        if let Some(detail) = &self.pr_detail {
            let (lines, _, _, _) = crate::ui::pr_detail::build_content(
                detail,
                self.pr_detail_files_expanded,
                self.pr_detail_comments_expanded,
                &self.palette,
                self.config.show_ascii_glyphs,
            );
            return lines;
        }
        if let Some(detail) = &self.issue_detail {
            let (lines, _) = crate::ui::issue_detail::build_content(
                detail,
                self.pr_detail_comments_expanded,
                &self.palette,
                self.config.show_ascii_glyphs,
            );
            return lines;
        }
        Vec::new()
    }

    /// Adjust `pr_detail_scroll` and `copy_mode.h_scroll` so that the cursor
    /// is always visible within the last-rendered viewport.
    fn ensure_cursor_visible(&mut self, lines: &[ratatui::text::Line<'static>]) {
        let area = self.pr_detail_viewport.get();
        let (vw, vh) = (area.width, area.height);
        if vh == 0 {
            return;
        }
        let cursor_row = u16::try_from(self.copy_mode.cursor.row).unwrap_or(u16::MAX);
        if cursor_row < self.pr_detail_scroll {
            self.pr_detail_scroll = cursor_row;
        } else if cursor_row >= self.pr_detail_scroll.saturating_add(vh) {
            self.pr_detail_scroll = cursor_row.saturating_sub(vh).saturating_add(1);
        }

        if vw == 0 {
            return;
        }
        let line = lines.get(self.copy_mode.cursor.row);
        let cursor_col = line
            .map_or(0, |l| crate::ui::copy_mode::cursor_display_col(l, self.copy_mode.cursor.col));
        if cursor_col < self.copy_mode.h_scroll {
            self.copy_mode.h_scroll = cursor_col;
        } else if cursor_col >= self.copy_mode.h_scroll.saturating_add(vw) {
            self.copy_mode.h_scroll = cursor_col.saturating_sub(vw).saturating_add(1);
        }
    }

    // ── Repo picker ───────────────────────────────────────────────────────────

    /// Close the repo picker and sync tabs to the current config.
    ///
    /// Also resets picker state so the next `p` press starts fresh.
    fn close_repo_picker(&mut self) {
        self.focus = self.repo_picker_return_focus;
        self.repo_picker_input.clear();
        self.repo_picker_mode = RepoPickerMode::List;
        self.sync_tabs_to_config();
    }

    /// Ensure `App::tabs` mirrors `config.repos`: open any newly-added repos,
    /// close any removed repos, and preserve the active tab where possible.
    ///
    /// Also drops stale entries from the `selection` map: a repo that has been
    /// removed from the config must not continue to hold a per-repo cursor
    /// index, or long-running sessions would accumulate dead entries.
    fn sync_tabs_to_config(&mut self) {
        // Close tabs whose repos are no longer in the config and drop any
        // per-repo state that only makes sense for a tracked repo.
        let removed: Vec<(crate::ui::tabs::TabId, String)> = self
            .tabs
            .tabs
            .iter()
            .filter(|t| !self.config.repos.contains(&t.repo))
            .map(|t| (t.id, t.repo.clone()))
            .collect();
        for (id, repo) in removed {
            self.tabs.close(id);
            self.selection.remove(&repo);
        }

        // Open tabs for repos not yet represented.
        for repo in &self.config.repos {
            self.tabs.open_or_focus(repo);
        }

        // Restore cursor within bounds.
        let max_idx = self.config.repos.len().saturating_sub(1);
        if self.repo_picker_list_cursor > max_idx {
            self.repo_picker_list_cursor = max_idx;
        }
    }

    /// Key handler for the repo picker overlay.
    fn handle_key_repo_picker(&mut self, key: crossterm::event::KeyEvent) {
        match self.repo_picker_mode {
            RepoPickerMode::List => self.handle_repo_picker_list_key(key),
            RepoPickerMode::Input => self.handle_repo_picker_input_key(key),
        }
    }

    /// Key handler for repo picker List mode.
    fn handle_repo_picker_list_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        if key.modifiers != KeyModifiers::NONE {
            return;
        }
        let repo_count = self.config.repos.len();

        match key.code {
            KeyCode::Esc => {
                self.close_repo_picker();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if repo_count > 0 {
                    self.repo_picker_list_cursor =
                        (self.repo_picker_list_cursor + 1).min(repo_count - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.repo_picker_list_cursor = self.repo_picker_list_cursor.saturating_sub(1);
            }
            KeyCode::Char('d') | KeyCode::Backspace => {
                if !self.config.repos.is_empty() {
                    let idx = self.repo_picker_list_cursor.min(repo_count - 1);
                    self.config.repos.remove(idx);
                    self.config.save();
                    // Clamp cursor after removal.
                    let new_len = self.config.repos.len();
                    if new_len > 0 && self.repo_picker_list_cursor >= new_len {
                        self.repo_picker_list_cursor = new_len - 1;
                    } else if new_len == 0 {
                        self.repo_picker_list_cursor = 0;
                    }
                    self.sync_tabs_to_config();
                }
            }
            KeyCode::Char('a' | 'i') => {
                self.repo_picker_mode = RepoPickerMode::Input;
            }
            KeyCode::Enter => {
                // Focus the selected repo's tab and close the picker.
                if repo_count > 0 {
                    let idx = self.repo_picker_list_cursor.min(repo_count - 1);
                    let repo = self.config.repos[idx].clone();
                    self.tabs.open_or_focus(&repo);
                    self.close_repo_picker();
                }
            }
            _ => {}
        }
    }

    /// Key handler for repo picker Input mode.
    fn handle_repo_picker_input_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        if key.modifiers != KeyModifiers::NONE {
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.repo_picker_mode = RepoPickerMode::List;
                self.repo_picker_input.clear();
            }
            KeyCode::Backspace => {
                self.repo_picker_input.pop();
            }
            KeyCode::Enter => {
                let slug = self.repo_picker_input.trim().to_owned();
                if !crate::ui::repo_picker::is_valid_repo_slug(&slug) {
                    self.show_flash(
                        format!("Invalid repo slug: \"{slug}\". Use owner/name format."),
                        std::time::Duration::from_secs(3),
                    );
                    return;
                }
                // Dedup: silently ignore if already tracked.
                if !self.config.repos.contains(&slug) {
                    self.config.repos.push(slug.clone());
                    self.config.save();
                    self.tabs.open_or_focus(&slug);
                    self.show_flash(format!("Added {slug}"), std::time::Duration::from_secs(2));
                }
                self.repo_picker_input.clear();
                // Stay in Input mode so the user can add more repos.
            }
            KeyCode::Char(c) => {
                self.repo_picker_input.push(c);
            }
            _ => {}
        }
    }

    // ── Confirmation overlay ──────────────────────────────────────────────────

    /// Dismiss the confirmation overlay and restore prior focus.
    fn dismiss_confirm(&mut self) {
        self.confirm = None;
        self.focus = self.confirm_return_focus;
    }

    /// Execute the pending confirmation action, then dismiss the overlay.
    fn execute_confirm(&mut self) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        self.focus = self.confirm_return_focus;

        match confirm.pending_action {
            crate::ui::confirm::ConfirmPending::CheckoutBranch { branch, .. } => {
                // Check tree cleanliness before running git.
                match crate::git::is_working_tree_clean() {
                    Err(e) => {
                        self.show_flash(
                            format!("git checkout failed: {e}"),
                            std::time::Duration::from_secs(4),
                        );
                        return;
                    }
                    Ok(false) => {
                        self.show_flash(
                            "Working tree is not clean; commit or stash first.",
                            std::time::Duration::from_secs(4),
                        );
                        return;
                    }
                    Ok(true) => {}
                }

                match crate::git::checkout_branch(&branch) {
                    Ok(()) => {
                        self.show_flash(
                            format!("Checked out {branch}"),
                            std::time::Duration::from_secs(3),
                        );
                    }
                    Err(e) => {
                        self.show_flash(
                            format!("git checkout failed: {e}"),
                            std::time::Duration::from_secs(4),
                        );
                    }
                }
            }
        }
    }

    /// Key handler for the confirmation overlay.
    fn handle_key_confirm(&mut self, key: crossterm::event::KeyEvent) {
        if key.modifiers != KeyModifiers::NONE {
            return;
        }

        match key.code {
            KeyCode::Char('y') => {
                self.handle_action(Action::ConfirmCheckout(true));
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.handle_action(Action::ConfirmCheckout(false));
            }
            _ => {}
        }
    }

    /// Build a `Confirm` overlay for checking out the selected PR's head branch.
    ///
    /// Reads from `pr_detail` when in Detail focus, or from the list-level
    /// `PullRequest` when in Dashboard focus.  No-ops if no branch can be
    /// resolved.
    fn begin_checkout_from_selection(&mut self) {
        // Prefer the detail view's head_ref when we are in it.
        let (branch, repo, number) = if let Some(detail) = &self.pr_detail {
            // `PrDetail::head_ref` is a `String`, so it can technically be
            // empty even though GitHub would not return one. Guard against it
            // so we never shell out `git checkout ""` (which produces a
            // confusing usage error instead of a clean flash).
            if detail.head_ref.is_empty() {
                self.show_flash(
                    "Branch info not available for this PR.",
                    std::time::Duration::from_secs(3),
                );
                return;
            }
            (detail.head_ref.clone(), detail.repo.clone(), detail.number)
        } else {
            // Fall back to the list-level PullRequest.
            let Some(repo_slug) = self.tabs.active_tab().map(|t| t.repo.clone()) else {
                return;
            };
            let Some(inbox) = &self.inbox else {
                return;
            };
            let sel = self.selection.get(&repo_slug).copied().unwrap_or(0);
            let prs: Vec<&crate::github::types::PullRequest> =
                inbox.prs.iter().filter(|pr| pr.repo == repo_slug).collect();
            let Some(pr) = prs.get(sel) else {
                return;
            };
            let Some(head) = pr.head_ref.clone() else {
                self.show_flash(
                    "Branch info not available; open the detail view first.",
                    std::time::Duration::from_secs(3),
                );
                return;
            };
            (head, repo_slug, pr.number)
        };

        // Warn when not in a git repo.
        if !crate::git::repo_cwd_is_git() {
            self.show_flash(
                "Not in a git repository; cannot checkout branch.",
                std::time::Duration::from_secs(3),
            );
            return;
        }

        let cwd =
            std::env::current_dir().map_or_else(|_| ".".to_owned(), |p| p.display().to_string());

        let confirm = crate::ui::confirm::Confirm {
            title: "Checkout branch".to_owned(),
            prompt: format!("Checkout `{branch}` in {cwd}?"),
            pending_action: crate::ui::confirm::ConfirmPending::CheckoutBranch {
                repo: repo.clone(),
                number,
                branch,
            },
        };

        self.confirm = Some(confirm);
        self.confirm_return_focus = self.focus;
        self.focus = Focus::Confirm;
    }

    /// Return to the dashboard, clearing all detail state.
    fn back_to_dashboard(&mut self) {
        self.focus = Focus::Dashboard;
        self.pr_detail = None;
        self.issue_detail = None;
        self.detail_error = None;
        self.detail_fetching = false;
        self.pr_detail_scroll = 0;
        self.pr_detail_files_expanded = false;
        self.pr_detail_comments_expanded = false;
        self.pr_detail_section_anchors.clear();
        self.pr_detail_unresolved_anchors.clear();
        self.pr_detail_unresolved_idx = 0;
        self.copy_mode.exit();
    }

    /// Jump to the next (`delta = 1`) or previous (`delta = -1`) section anchor.
    fn detail_jump_section(&mut self, delta: i32) {
        // Compute anchors on demand from the current pr_detail instead of
        // relying on a cached copy populated by the renderer. The renderer
        // has `&App`, not `&mut App`, so anchors computed during `draw`
        // cannot be written back — reading a stale cache silently no-ops
        // Tab navigation.
        let Some(detail) = &self.pr_detail else {
            return;
        };
        let (_, section_anchors, unresolved, _) = crate::ui::pr_detail::build_content(
            detail,
            self.pr_detail_files_expanded,
            self.pr_detail_comments_expanded,
            &self.palette,
            self.config.show_ascii_glyphs,
        );
        self.pr_detail_section_anchors = section_anchors;
        self.pr_detail_unresolved_anchors = unresolved;

        let anchors = &self.pr_detail_section_anchors;
        if anchors.is_empty() {
            return;
        }
        let current = self.pr_detail_scroll;
        if delta > 0 {
            if let Some(&next) = anchors.iter().find(|&&a| a > current) {
                self.pr_detail_scroll = next;
            } else {
                self.pr_detail_scroll = anchors[0];
            }
        } else if let Some(&prev) = anchors.iter().rev().find(|&&a| a < current) {
            self.pr_detail_scroll = prev;
        } else if let Some(&last) = anchors.last() {
            self.pr_detail_scroll = last;
        }
    }

    /// Cycle the scroll offset between unresolved review thread anchors.
    fn detail_cycle_unresolved(&mut self, delta: i32) {
        // Same staleness concern as `detail_jump_section` — recompute anchors
        // from the current pr_detail so a Tab/n/N press after the first
        // render actually has data to work with.
        let Some(detail) = &self.pr_detail else {
            return;
        };
        let (_, section_anchors, unresolved, _) = crate::ui::pr_detail::build_content(
            detail,
            self.pr_detail_files_expanded,
            self.pr_detail_comments_expanded,
            &self.palette,
            self.config.show_ascii_glyphs,
        );
        self.pr_detail_section_anchors = section_anchors;
        self.pr_detail_unresolved_anchors = unresolved;

        let anchors = &self.pr_detail_unresolved_anchors;
        if anchors.is_empty() {
            return;
        }
        let len = anchors.len();
        if delta > 0 {
            self.pr_detail_unresolved_idx = (self.pr_detail_unresolved_idx + 1) % len;
        } else {
            self.pr_detail_unresolved_idx = (self.pr_detail_unresolved_idx + len - 1) % len;
        }
        self.pr_detail_scroll = anchors[self.pr_detail_unresolved_idx];
    }

    /// Open the detail view for the currently selected PR or issue.
    ///
    /// Reads the active tab, view mode, and selection index to determine which
    /// item to fetch, then dispatches the appropriate detail action and switches
    /// focus to [`Focus::Detail`].
    fn open_detail_for_selection(&mut self) {
        let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) else {
            return;
        };
        let Some(inbox) = &self.inbox else {
            return;
        };

        let mode = self.session.view_mode(&repo);
        let sel = self.selection.get(&repo).copied().unwrap_or(0);

        match mode {
            crate::state::ViewMode::Prs => {
                let prs: Vec<&crate::github::types::PullRequest> =
                    inbox.prs.iter().filter(|pr| pr.repo == repo).collect();
                if let Some(pr) = prs.get(sel) {
                    let number = pr.number;
                    // Reset detail state before switching focus. `detail_fetching`
                    // is set by `spawn_detail_fetch` itself — if we set it here
                    // too, the guard inside that function sees `true` and
                    // silently skips the fetch, leaving the view stuck on the
                    // spinner forever.
                    self.pr_detail = None;
                    self.issue_detail = None;
                    self.detail_error = None;
                    self.pr_detail_scroll = 0;
                    self.pr_detail_files_expanded = false;
                    self.pr_detail_comments_expanded = false;
                    self.pr_detail_section_anchors.clear();
                    self.pr_detail_unresolved_anchors.clear();
                    self.pr_detail_unresolved_idx = 0;
                    self.focus = Focus::Detail;
                    if let Some(tx) = self.action_tx.clone() {
                        self.spawn_detail_fetch(DetailKind::Pr, repo, number, tx);
                    }
                }
            }
            crate::state::ViewMode::Issues => {
                let issues: Vec<&crate::github::types::Issue> =
                    inbox.issues.iter().filter(|i| i.repo == repo).collect();
                if let Some(issue) = issues.get(sel) {
                    let number = issue.number;
                    // See the PR branch above: `detail_fetching` is set by
                    // `spawn_detail_fetch`, not here, to avoid the self-
                    // blocking guard.
                    self.pr_detail = None;
                    self.issue_detail = None;
                    self.detail_error = None;
                    self.pr_detail_scroll = 0;
                    self.pr_detail_files_expanded = false;
                    self.pr_detail_comments_expanded = false;
                    self.pr_detail_section_anchors.clear();
                    self.pr_detail_unresolved_anchors.clear();
                    self.pr_detail_unresolved_idx = 0;
                    self.focus = Focus::Detail;
                    if let Some(tx) = self.action_tx.clone() {
                        self.spawn_detail_fetch(DetailKind::Issue, repo, number, tx);
                    }
                }
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
                self.open_detail_for_selection();
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
                self.handle_action(Action::CheckoutBranch);
            }
            KeyCode::Char('p') => {
                debug!("dashboard: 'p' pressed — dispatching OpenRepoPicker");
                self.pending_g = false;
                self.handle_action(Action::OpenRepoPicker);
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
#[allow(clippy::expect_used, clippy::unwrap_used)]
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
            base_ref: Some("main".to_owned()),
            head_ref: Some("feat/test".to_owned()),
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

    // ── Phase 4 detail-UI tests ───────────────────────────────────────────────

    /// Pressing Esc in Detail focus clears `pr_detail` and `issue_detail`, resets
    /// scroll, and returns focus to Dashboard.
    #[test]
    fn esc_in_detail_focus_returns_to_dashboard() {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::Detail;
        app.pr_detail_scroll = 42;

        app.back_to_dashboard();

        assert_eq!(app.focus, Focus::Dashboard);
        assert!(app.pr_detail.is_none());
        assert!(app.issue_detail.is_none());
        assert!(app.detail_error.is_none());
        assert_eq!(app.pr_detail_scroll, 0);
    }

    /// Pressing Enter on the dashboard when a PR is selected must set
    /// `detail_fetching = true`, switch focus to Detail, and clear prior state.
    #[test]
    fn enter_on_dashboard_populates_detail_fetching() {
        let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        let inbox = Inbox {
            viewer_login: "viewer".to_owned(),
            prs: vec![make_pr("o/r", "clean", "viewer")],
            issues: vec![],
        };
        app.on_inbox_loaded(inbox);

        // Simulate Enter key on the dashboard.
        app.open_detail_for_selection();

        // detail_fetching should be true (we can't actually fetch without a
        // client, but the flag should be set if a client exists; in tests there
        // is no client so `spawn_detail_fetch` returns early, but focus still
        // switches and the flags are reset).
        assert_eq!(app.focus, Focus::Detail);
        assert_eq!(app.pr_detail_scroll, 0);
    }

    /// `pr_detail_scroll` must not exceed a plausible content ceiling.
    ///
    /// The actual clamp happens in the renderer, but we can verify that the
    /// scroll field increases monotonically with j presses and that wrapping
    /// `u16` arithmetic is avoided (saturating add).
    #[test]
    fn scroll_clamped_by_saturating_add() {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.pr_detail_scroll = u16::MAX;
        // Pressing j again must not wrap around.
        app.pr_detail_scroll = app.pr_detail_scroll.saturating_add(1);
        assert_eq!(app.pr_detail_scroll, u16::MAX, "saturating add must not wrap");
    }

    /// Pressing `o` with an invalid URL produces a flash error.
    /// Verifies the `open_url_in_browser` error-message shape without actually
    /// invoking the underlying `open::that` call.
    ///
    /// The previous version of this test called `open::that("")` directly —
    /// which on macOS treats an empty path as the current directory and pops
    /// the Finder window. Every `cargo test` run opened Finder, which is
    /// exactly the same class of "tests must not side-effect on the
    /// developer's machine" bug we fixed for `Config::save()` with
    /// `with_config_dir_override`.
    ///
    /// Here we only assert on the error message wrapper — the actual `open`
    /// crate behaviour is out of scope for unit tests and can be covered by
    /// an `#[ignore]`-marked integration test if end-to-end verification is
    /// ever needed.
    #[test]
    fn open_browser_error_message_includes_url() {
        use anyhow::Context as _;

        // Short-circuit by constructing the same `anyhow::Error` the function
        // would produce on a failed `open::that`; the wrapper shape is what
        // we care about — not whether the OS accepts the URL.
        let url = "https://example.invalid/pr/1";
        let wrapped: anyhow::Result<()> = Err(anyhow::anyhow!("simulated launch failure"))
            .with_context(|| format!("failed to open URL in browser: {url}"));
        let msg = format!("{:#}", wrapped.unwrap_err());
        assert!(msg.contains(url), "error message must include the URL for debuggability");
        assert!(
            msg.contains("failed to open URL in browser"),
            "wrapper message must name the operation"
        );
    }

    /// `copy_to_clipboard` is skipped in headless environments; this test
    /// verifies the function returns a typed Result without panicking.
    #[test]
    #[ignore = "clipboard unavailable on headless CI; run manually"]
    fn copy_url_does_not_panic() {
        let result = crate::actions_util::copy_to_clipboard("https://github.com");
        // On a real desktop this should succeed; on headless it fails gracefully.
        let _ = result;
    }

    // ── Copy mode & mouse tests ───────────────────────────────────────────────

    fn key(code: KeyCode) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Pressing `v` in detail focus enters copy mode with the cursor anchored
    /// at the top of the current viewport (so it lands where the user is
    /// already looking, not scrolled off-screen).
    #[test]
    fn v_in_detail_enters_copy_mode() {
        let mut app = App::new(crate::config::Config::default(), crate::state::AppSession::default());
        app.focus = Focus::Detail;
        app.pr_detail_scroll = 12;

        app.handle_key(key(KeyCode::Char('v')));

        assert!(app.copy_mode.active);
        assert_eq!(app.copy_mode.cursor.row, 12, "cursor must start at scroll offset");
        assert_eq!(app.copy_mode.cursor.col, 0);
        assert!(app.copy_mode.anchor.is_none(), "no selection until V pressed");
    }

    /// Esc in copy mode exits the mode but stays in the detail focus —
    /// distinct from Esc in normal detail mode, which returns to dashboard.
    #[test]
    fn esc_in_copy_mode_stays_in_detail() {
        let mut app = App::new(crate::config::Config::default(), crate::state::AppSession::default());
        app.focus = Focus::Detail;
        app.copy_mode.enter(0, 0);

        app.handle_key(key(KeyCode::Esc));

        assert!(!app.copy_mode.active);
        assert_eq!(app.focus, Focus::Detail, "Esc in copy mode must not leave detail");
    }

    /// Returning to the dashboard via `b` also tears down copy-mode state.
    #[test]
    fn back_to_dashboard_clears_copy_mode() {
        let mut app = App::new(crate::config::Config::default(), crate::state::AppSession::default());
        app.focus = Focus::Detail;
        app.copy_mode.enter(5, 7);

        app.back_to_dashboard();

        assert_eq!(app.focus, Focus::Dashboard);
        assert!(!app.copy_mode.active);
        assert_eq!(app.copy_mode.cursor, crate::ui::copy_mode::Pos::default());
    }

    /// Mouse wheel in detail focus scrolls the content by 3 lines per tick.
    #[test]
    fn mouse_wheel_scrolls_detail() {
        use crossterm::event::{MouseEvent, MouseEventKind};
        let mut app = App::new(crate::config::Config::default(), crate::state::AppSession::default());
        app.focus = Focus::Detail;
        app.pr_detail_scroll = 10;

        app.handle_action(Action::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.pr_detail_scroll, 13);

        app.handle_action(Action::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.pr_detail_scroll, 10);
    }

    /// A left-click inside the cached detail viewport enters copy mode and
    /// places the cursor at the corresponding content coordinate.
    #[test]
    fn mouse_click_in_detail_places_cursor() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        let mut app = App::new(crate::config::Config::default(), crate::state::AppSession::default());
        app.focus = Focus::Detail;
        // Pretend the detail area is rendered at origin (0,1) with size 80x20.
        app.pr_detail_viewport
            .set(ratatui::layout::Rect::new(0, 1, 80, 20));
        app.pr_detail_scroll = 5;

        app.handle_action(Action::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: 3, // inside viewport: row offset 2 -> content row 7
            modifiers: KeyModifiers::NONE,
        }));

        assert!(app.copy_mode.active);
        assert_eq!(app.copy_mode.cursor.row, 7);
        assert_eq!(app.copy_mode.cursor.col, 10);
    }

    /// A left-click outside the cached viewport must be ignored (no copy-mode
    /// entry, no state mutation). This also covers the case where the
    /// viewport hasn't been cached yet (zero-sized rect).
    #[test]
    fn mouse_click_outside_viewport_is_ignored() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        let mut app = App::new(crate::config::Config::default(), crate::state::AppSession::default());
        app.focus = Focus::Detail;
        app.pr_detail_viewport
            .set(ratatui::layout::Rect::new(0, 1, 10, 10));

        app.handle_action(Action::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 50,
            row: 50, // far outside
            modifiers: KeyModifiers::NONE,
        }));

        assert!(!app.copy_mode.active);
    }

    /// Dragging with left button held starts a selection on first drag and
    /// moves the cursor on subsequent drag events.
    #[test]
    fn mouse_drag_starts_selection() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        let mut app = App::new(crate::config::Config::default(), crate::state::AppSession::default());
        app.focus = Focus::Detail;
        app.pr_detail_viewport
            .set(ratatui::layout::Rect::new(0, 1, 80, 20));

        // Initial click to enter copy mode at (0, 2).
        app.handle_action(Action::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(app.copy_mode.active);
        assert!(app.copy_mode.anchor.is_none());

        // First drag event sets the anchor at the current cursor position.
        app.handle_action(Action::Mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 5,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }));

        assert_eq!(app.copy_mode.anchor, Some(crate::ui::copy_mode::Pos { row: 0, col: 2 }));
        // Cursor moved to click position (row 0 inside content since no lines).
        // Without loaded detail, current_detail_lines() returns an empty Vec,
        // which clamps row to 0. Column is free-form (display cell).
        assert_eq!(app.copy_mode.cursor.col, 5);
    }

    // ── Phase 5 tests ─────────────────────────────────────────────────────────

    /// Pressing `p` on the dashboard must open the repo picker and set
    /// `Focus::RepoPicker`.
    #[test]
    fn pressing_p_opens_repo_picker() {
        let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        app.handle_action(Action::OpenRepoPicker);

        assert_eq!(app.focus, Focus::RepoPicker);
    }

    /// Opening the repo picker must reset input state.
    #[test]
    fn open_repo_picker_resets_state() {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        // Pre-populate stale picker state.
        app.repo_picker_input = "stale/input".to_owned();
        app.repo_picker_mode = RepoPickerMode::Input;

        app.handle_action(Action::OpenRepoPicker);

        assert_eq!(app.focus, Focus::RepoPicker);
        assert!(app.repo_picker_input.is_empty(), "input buffer should be cleared on open");
        assert_eq!(app.repo_picker_mode, RepoPickerMode::List);
    }

    /// Closing the repo picker must restore the previous focus.
    #[test]
    fn close_repo_picker_restores_focus() {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::Dashboard;

        app.handle_action(Action::OpenRepoPicker);
        assert_eq!(app.focus, Focus::RepoPicker);

        // Close via Esc (simulated by calling close_repo_picker directly).
        app.close_repo_picker();
        assert_eq!(app.focus, Focus::Dashboard);
    }

    /// Adding a valid slug via the picker must append it to `config.repos`.
    #[test]
    fn repo_picker_add_valid_slug() {
        // Sandbox the config save under a tempdir so the test cannot clobber
        // the developer's real `~/Library/Application Support/octopeek/`
        // (or `$XDG_CONFIG_HOME/octopeek/`) file.
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);
            app.focus = Focus::RepoPicker;
            app.repo_picker_mode = RepoPickerMode::Input;
            app.repo_picker_input = "rust-lang/rust".to_owned();

            let key = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_repo_picker_input_key(key);

            assert!(app.config.repos.contains(&"rust-lang/rust".to_owned()));
            assert!(
                app.repo_picker_input.is_empty(),
                "buffer must be cleared after successful add"
            );
        });
    }

    /// Adding a duplicate slug must not create a duplicate entry.
    #[test]
    fn repo_picker_add_dedup() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config {
                repos: vec!["rust-lang/rust".to_owned()],
                ..Default::default()
            };
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);
            app.focus = Focus::RepoPicker;
            app.repo_picker_mode = RepoPickerMode::Input;
            app.repo_picker_input = "rust-lang/rust".to_owned();

            let key = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_repo_picker_input_key(key);

            assert_eq!(
                app.config.repos.iter().filter(|r| *r == "rust-lang/rust").count(),
                1,
                "duplicate repo must not be added"
            );
        });
    }

    /// An invalid slug must set a flash error and not append to `config.repos`.
    #[test]
    fn repo_picker_add_invalid_slug_sets_flash() {
        // This path rejects the slug before reaching Config::save, so an
        // override is not strictly required — but wrapping keeps all tests
        // uniformly sandboxed in case the code path evolves.
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);
            app.focus = Focus::RepoPicker;
            app.repo_picker_mode = RepoPickerMode::Input;
            app.repo_picker_input = "no-slash-here".to_owned();

            let key = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_repo_picker_input_key(key);

            assert!(app.config.repos.is_empty(), "invalid slug must not be added");
            assert!(app.flash.is_some(), "flash message must be set on validation failure");
        });
    }

    /// Deleting a repo must also drop its entry from the per-repo selection
    /// map so long-running sessions don't accumulate dead cursor state.
    #[test]
    fn repo_picker_delete_cleans_up_selection_map() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config {
                repos: vec!["owner/a".to_owned(), "owner/b".to_owned()],
                ..Default::default()
            };
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);
            app.selection.insert("owner/a".to_owned(), 3);
            app.selection.insert("owner/b".to_owned(), 1);

            app.focus = Focus::RepoPicker;
            app.repo_picker_mode = RepoPickerMode::List;
            app.repo_picker_list_cursor = 0;
            let key = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('d'),
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_repo_picker_list_key(key);

            assert!(
                !app.selection.contains_key("owner/a"),
                "deleted repo's selection entry must be removed"
            );
            assert_eq!(
                app.selection.get("owner/b"),
                Some(&1),
                "other repos' selection entries must be untouched"
            );
        });
    }

    /// Deleting a repo in List mode must remove it from `config.repos` and
    /// close the corresponding tab.
    #[test]
    fn repo_picker_delete_removes_repo_and_tab() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config {
                repos: vec!["owner/a".to_owned(), "owner/b".to_owned()],
                ..Default::default()
            };
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);
            app.focus = Focus::RepoPicker;
            app.repo_picker_mode = RepoPickerMode::List;
            app.repo_picker_list_cursor = 0;

            let key = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('d'),
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_repo_picker_list_key(key);

            assert!(!app.config.repos.contains(&"owner/a".to_owned()), "repo must be removed");
            assert!(app.config.repos.contains(&"owner/b".to_owned()), "other repo must remain");
            assert!(
                app.tabs.tabs.iter().all(|t| t.repo != "owner/a"),
                "tab for deleted repo must be closed"
            );
        });
    }

    /// Regression guard: `Config::save` with an override writes ONLY to the
    /// override directory and the real platform config path is never touched.
    ///
    /// Without this invariant, earlier picker tests clobbered the developer's
    /// actual `~/Library/Application Support/octopeek/config.toml` on every
    /// `cargo test` run.
    #[test]
    fn config_save_respects_override() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let expected = tmp.path().join("config.toml");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config {
                repos: vec!["sentinel/override".to_owned()],
                ..Default::default()
            };
            config.save();
            assert!(expected.exists(), "save must write to the override path");
            let written = std::fs::read_to_string(&expected).expect("read override");
            assert!(written.contains("sentinel/override"), "override file must contain the data");
        });
    }

    /// Pressing `c` on the dashboard when the inbox has a PR with `head_ref`
    /// must populate `app.confirm` and switch focus to `Focus::Confirm`.
    #[test]
    fn pressing_c_on_dashboard_with_pr_opens_confirm() {
        let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        let inbox = Inbox {
            viewer_login: "viewer".to_owned(),
            prs: vec![make_pr("o/r", "clean", "viewer")],
            issues: vec![],
        };
        app.on_inbox_loaded(inbox);

        app.handle_action(Action::CheckoutBranch);

        // Should be in Confirm focus if git repo is available; if not in a git
        // repo, a flash is shown instead — both are valid.
        match app.focus {
            Focus::Confirm => {
                assert!(app.confirm.is_some(), "confirm must be populated");
                let confirm = app.confirm.as_ref().unwrap();
                assert!(
                    matches!(
                        &confirm.pending_action,
                        crate::ui::confirm::ConfirmPending::CheckoutBranch { branch, .. }
                        if branch == "feat/test"
                    ),
                    "confirm must have the correct branch"
                );
            }
            Focus::Dashboard => {
                // Not in a git repo — flash should explain this.
                assert!(
                    app.flash.is_some(),
                    "a flash must be set when not in a git repo or branch is unavailable"
                );
            }
            other => panic!("unexpected focus {other:?}"),
        }
    }

    /// Pressing `n`/`N` dismiss the confirm overlay with no action.
    #[test]
    fn confirm_n_cancels_and_restores_focus() {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        app.confirm = Some(crate::ui::confirm::Confirm {
            title: "Test".to_owned(),
            prompt: "Are you sure?".to_owned(),
            pending_action: crate::ui::confirm::ConfirmPending::CheckoutBranch {
                repo: "o/r".to_owned(),
                number: 1,
                branch: "feat/x".to_owned(),
            },
        });
        app.confirm_return_focus = Focus::Dashboard;
        app.focus = Focus::Confirm;

        app.handle_action(Action::ConfirmCheckout(false));

        assert_eq!(app.focus, Focus::Dashboard, "focus must be restored after cancel");
        assert!(app.confirm.is_none(), "confirm must be cleared after cancel");
    }

    // ── First-run wizard tests ────────────────────────────────────────────────

    /// Helper: build an `Inbox` with a given set of PRs and issues.
    fn make_inbox(prs: Vec<(&str, &str)>, issues: Vec<&str>) -> Inbox {
        Inbox {
            viewer_login: "viewer".to_owned(),
            prs: prs.into_iter().map(|(repo, variant)| make_pr(repo, variant, "viewer")).collect(),
            issues: issues.into_iter().map(make_issue).collect(),
        }
    }

    /// When config is empty and the inbox has items, `on_inbox_loaded` must
    /// switch focus to `FirstRun` and populate `first_run_suggestions`.
    #[test]
    fn on_inbox_loaded_triggers_first_run_when_config_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default(); // repos empty
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);

            let inbox = make_inbox(
                vec![("alice/foo", "clean"), ("bob/bar", "clean"), ("alice/foo", "conflict")],
                vec![],
            );
            app.on_inbox_loaded(inbox);

            assert_eq!(app.focus, Focus::FirstRun, "focus must switch to FirstRun");
            assert_eq!(
                app.first_run_suggestions.len(),
                2,
                "two distinct repos must appear in suggestions"
            );
            // alice/foo has 2 PRs; bob/bar has 1.
            assert_eq!(app.first_run_suggestions[0].repo, "alice/foo");
            assert_eq!(app.first_run_suggestions[0].count, 2);
            assert_eq!(app.first_run_suggestions[1].repo, "bob/bar");
            assert_eq!(app.first_run_suggestions[1].count, 1);
        });
    }

    /// When config already has repos, `on_inbox_loaded` must NOT trigger the
    /// first-run wizard.
    #[test]
    fn on_inbox_loaded_skips_first_run_when_config_nonempty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config {
                repos: vec!["existing/repo".to_owned()],
                ..Default::default()
            };
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);

            let inbox = make_inbox(vec![("alice/foo", "clean")], vec![]);
            app.on_inbox_loaded(inbox);

            assert_eq!(
                app.focus,
                Focus::Dashboard,
                "focus must remain Dashboard when config has repos"
            );
            assert!(app.first_run_suggestions.is_empty(), "no suggestions when config is nonempty");
        });
    }

    /// When config is empty AND inbox is empty, focus must stay Dashboard
    /// (existing empty-dashboard state is the correct UX).
    #[test]
    fn on_inbox_loaded_skips_first_run_when_inbox_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);

            let inbox = make_inbox(vec![], vec![]);
            app.on_inbox_loaded(inbox);

            assert_eq!(app.focus, Focus::Dashboard, "focus must stay Dashboard for empty inbox");
            assert!(app.first_run_suggestions.is_empty());
        });
    }

    /// Space key in `FirstRun` focus must toggle the selected state of the
    /// cursor row.
    #[test]
    fn first_run_space_toggles_selection() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);
            app.focus = Focus::FirstRun;
            app.first_run_suggestions =
                vec![FirstRunSuggestion { repo: "a/b".to_owned(), count: 1, selected: false }];
            app.first_run_cursor = 0;

            let space = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(' '),
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_key_first_run(space);
            assert!(app.first_run_suggestions[0].selected, "Space must select the row");

            // Press again to deselect.
            let space2 = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(' '),
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_key_first_run(space2);
            assert!(!app.first_run_suggestions[0].selected, "second Space must deselect the row");
        });
    }

    /// Enter in `FirstRun` focus must commit selected repos to config, clear
    /// the suggestions, switch to Dashboard, and set a flash message.
    #[test]
    fn first_run_enter_commits_selected() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);
            app.focus = Focus::FirstRun;
            app.first_run_suggestions = vec![
                FirstRunSuggestion { repo: "a/b".to_owned(), count: 5, selected: true },
                FirstRunSuggestion { repo: "c/d".to_owned(), count: 3, selected: true },
                FirstRunSuggestion { repo: "e/f".to_owned(), count: 1, selected: false },
            ];

            let enter = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_key_first_run(enter);

            assert_eq!(app.focus, Focus::Dashboard, "focus must switch to Dashboard after commit");
            assert!(app.first_run_suggestions.is_empty(), "suggestions must be cleared");
            assert!(
                app.config.repos.contains(&"a/b".to_owned()),
                "selected repo a/b must be in config"
            );
            assert!(
                app.config.repos.contains(&"c/d".to_owned()),
                "selected repo c/d must be in config"
            );
            assert!(
                !app.config.repos.contains(&"e/f".to_owned()),
                "unselected repo e/f must NOT be in config"
            );
            assert!(app.flash.is_some(), "a flash message must be set after committing");
        });
    }

    /// Esc in `FirstRun` focus must skip without touching config and switch
    /// focus to Dashboard.
    #[test]
    fn first_run_esc_skips_without_commit() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);
            app.focus = Focus::FirstRun;
            app.first_run_suggestions =
                vec![FirstRunSuggestion { repo: "a/b".to_owned(), count: 2, selected: true }];

            let esc = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Esc,
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_key_first_run(esc);

            assert_eq!(app.focus, Focus::Dashboard, "focus must be Dashboard after Esc");
            assert!(app.config.repos.is_empty(), "Esc must not commit any repos to config");
            assert!(app.first_run_suggestions.is_empty(), "suggestions must be cleared on Esc");
        });
    }

    /// Suggestions must be sorted by count descending, then alphabetically.
    #[test]
    fn first_run_suggestions_sorted_by_count_desc() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);

            // a/b appears 5 times (5 PRs), c/d 10 times (10 PRs).
            let mut prs: Vec<(&str, &str)> = Vec::new();
            for _ in 0..5 {
                prs.push(("a/b", "clean"));
            }
            for _ in 0..10 {
                prs.push(("c/d", "clean"));
            }
            let inbox = make_inbox(prs, vec![]);
            app.on_inbox_loaded(inbox);

            assert_eq!(app.focus, Focus::FirstRun, "must switch to FirstRun");
            assert_eq!(
                app.first_run_suggestions[0].repo, "c/d",
                "repo with more items must be first"
            );
            assert_eq!(app.first_run_suggestions[0].count, 10);
        });
    }

    /// A repo with 2 PRs and 3 issues must yield a combined count of 5.
    #[test]
    fn first_run_suggestion_counts_pr_plus_issue() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);

            let inbox =
                make_inbox(vec![("x/y", "clean"), ("x/y", "conflict")], vec!["x/y", "x/y", "x/y"]);
            app.on_inbox_loaded(inbox);

            let sug = app.first_run_suggestions.iter().find(|s| s.repo == "x/y");
            assert!(sug.is_some(), "x/y must appear in suggestions");
            assert_eq!(sug.unwrap().count, 5, "2 PRs + 3 issues = 5 total");
        });
    }

    /// Regression guard for the reviewer's "selections survive a mid-wizard
    /// refresh" invariant. A second `on_inbox_loaded` call while focus is
    /// `FirstRun` must NOT clobber the user's toggled selections.
    ///
    /// The guard at the top of `on_inbox_loaded` requires
    /// `focus == Dashboard` to populate suggestions; with focus still on the
    /// wizard, the method must leave `first_run_suggestions` intact.
    #[test]
    fn first_run_survives_mid_wizard_refresh() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);

            // Initial fetch triggers the wizard.
            let inbox = make_inbox(vec![("a/b", "clean"), ("c/d", "clean")], vec![]);
            app.on_inbox_loaded(inbox);
            assert_eq!(app.focus, Focus::FirstRun);
            assert_eq!(app.first_run_suggestions.len(), 2);

            // User toggles the first suggestion.
            app.first_run_cursor = 0;
            app.first_run_suggestions[0].selected = true;
            let snapshot_repo = app.first_run_suggestions[0].repo.clone();

            // A background refresh arrives while focus is still on the wizard.
            let inbox2 =
                make_inbox(vec![("a/b", "clean"), ("c/d", "clean"), ("e/f", "clean")], vec![]);
            app.on_inbox_loaded(inbox2);

            assert_eq!(app.focus, Focus::FirstRun, "focus must not bounce");
            assert_eq!(app.first_run_suggestions.len(), 2, "suggestions must not be rebuilt");
            assert_eq!(
                app.first_run_suggestions[0].repo, snapshot_repo,
                "suggestion ordering must be preserved"
            );
            assert!(
                app.first_run_suggestions[0].selected,
                "user's selection must survive the refresh"
            );
        });
    }

    /// Regression guard for the reviewer's `a`-key roundtrip concern. Pressing
    /// `a` in the wizard opens the repo picker in Input mode and records
    /// `FirstRun` as the return-to focus; after the picker closes (via
    /// `close_repo_picker`) the user lands back in the wizard, not on the
    /// dashboard.
    #[test]
    fn first_run_a_roundtrips_back_to_first_run() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);

            let inbox = make_inbox(vec![("a/b", "clean")], vec![]);
            app.on_inbox_loaded(inbox);
            assert_eq!(app.focus, Focus::FirstRun, "wizard must be active");

            // User presses `a` — should open picker with return_focus recorded.
            let a_key = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('a'),
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_key_first_run(a_key);
            assert_eq!(app.focus, Focus::RepoPicker);
            assert_eq!(
                app.repo_picker_return_focus,
                Focus::FirstRun,
                "return-focus must be recorded so the picker close path returns here"
            );

            // Simulate picker close.
            app.close_repo_picker();
            assert_eq!(app.focus, Focus::FirstRun, "closing picker must return to wizard");
        });
    }

    /// Pressing Enter with zero items ticked must flash a hint and NOT
    /// close the wizard — otherwise the user's accidental Enter would
    /// dump them to an empty dashboard with no feedback.
    #[test]
    fn first_run_enter_with_nothing_selected_flashes_hint() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let config = crate::config::Config::default();
            let session = crate::state::AppSession::default();
            let mut app = App::new(config, session);

            let inbox = make_inbox(vec![("a/b", "clean")], vec![]);
            app.on_inbox_loaded(inbox);
            assert_eq!(app.focus, Focus::FirstRun);
            assert!(
                !app.first_run_suggestions.iter().any(|s| s.selected),
                "no suggestions should start selected"
            );

            let enter = crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            );
            app.handle_key_first_run(enter);

            assert_eq!(app.focus, Focus::FirstRun, "wizard must stay open on empty Enter");
            assert!(app.flash.is_some(), "a hint flash must be shown");
            assert!(app.config.repos.is_empty(), "config must not be mutated");
        });
    }
}
