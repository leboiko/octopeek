//! [`App`] struct definition, constructor, and single-field accessor methods.

use chrono::{DateTime, Utc};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use crate::config::Config;
use crate::github;
use crate::state::AppSession;
use crate::theme::Palette;
use crate::ui::pr_detail::DetailSection;
use crate::ui::tabs::Tabs;

use super::actions::Action;
use super::types::{FirstRunSuggestion, Focus, PerTabState, RepoPickerMode};

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
    pub refresh_handle: Option<JoinHandle<()>>,

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
    /// Per-repo-tab state snapshot used to restore the user's view when they
    /// switch tabs and come back. Key: repo slug. See [`PerTabState`] for the
    /// shape and the restore policy (re-fetch, don't cache the payload).
    pub per_tab_state: HashMap<String, PerTabState>,
    /// When the inbox was last successfully loaded (used to display "last synced" text).
    pub inbox_loaded_at: Option<DateTime<Utc>>,
    /// `true` when the user pressed `g` and is waiting for a second `g` (vim-style gg).
    pub pending_g: bool,
    /// Transient message displayed in the status bar for a short duration.
    ///
    /// Set via [`App::show_flash`]; the status bar renderer clears it once
    /// `FlashMessage::is_active` returns `false`.
    pub flash: Option<crate::ui::status_bar::FlashMessage>,

    // ── Detail state ──���───────────────────────────────────────────────────────
    /// Most-recently-fetched PR detail, absent until a successful detail fetch.
    pub pr_detail: Option<github::detail::PrDetail>,
    /// Most-recently-fetched issue detail, absent until a successful detail fetch.
    pub issue_detail: Option<github::detail::IssueDetail>,
    /// `true` while a background detail fetch is in-flight (cold-miss spinner).
    pub detail_fetching: bool,
    /// Human-readable description of the last detail fetch error, if any.
    pub detail_error: Option<String>,
    /// In-process LRU-less cache of PR and issue detail payloads.
    ///
    /// Populated whenever a detail arrives from GitHub; read back by
    /// `restore_active_tab_state` to serve stale-while-revalidate hits.
    pub detail_cache: github::DetailCache,
    /// When `Some((repo, number))`, a background SWR re-fetch is in flight for
    /// that detail. Used to avoid duplicate fetches and to surface the
    /// "refreshing…" indicator in the status bar.
    pub detail_refreshing: Option<(String, u32)>,
    /// Per-section vertical scroll offsets for the PR detail right pane.
    ///
    /// Switching sections preserves each section's individual scroll position
    /// so the user returns to where they left off.
    pub pr_detail_scroll: HashMap<DetailSection, u16>,
    /// Scroll offset per file for the diff view. Separate from
    /// `pr_detail_scroll` so that cycling `J`/`K` between files preserves
    /// each file's scroll position independently. Key: file path.
    pub pr_detail_diff_scroll: HashMap<String, u16>,
    /// `true` when the files section in the PR detail view is fully expanded.
    pub pr_detail_files_expanded: bool,
    /// `true` when the comments section in the detail view is fully expanded.
    /// Shared between PR and issue detail — both views are mutually exclusive.
    pub detail_comments_expanded: bool,
    /// Lookup table mapping `(file, line)` to the review threads anchored
    /// there. Rebuilt whenever a new `PrDetail` arrives; cleared alongside
    /// `pr_detail` in `clear_detail_state` so stale indices can't race
    /// with fresh fetches. `None` when no PR detail is loaded.
    pub thread_index: Option<crate::ui::pr_detail::ThreadIndex>,
    /// Whether outdated review threads are rendered in the Comments section.
    ///
    /// Defaults to `true` (visible-but-muted, split under a dashed
    /// `OUTDATED` divider). The user can toggle to hide them with `z` —
    /// the disclosure row remains so the presence of outdated threads is
    /// never silently dropped. Ephemeral; not persisted across sessions.
    pub detail_show_outdated: bool,
    /// Currently selected section in the PR detail sidebar.
    pub pr_detail_selected_section: DetailSection,
    /// Index of the highlighted file in the sidebar files list.
    ///
    /// Set when clicking a file row; Phase 2 will use this to open the diff view.
    pub pr_detail_files_cursor: usize,
    /// `true` when the Files section should display the unified diff rather than
    /// the one-line-per-file overview. Defaults to `false` (overview). Set to
    /// `true` by `F` or by clicking a sidebar file row; reset to `false` by `$`
    /// or `back_to_dashboard`.
    pub pr_detail_files_show_diff: bool,
    /// Width of the sidebar in columns. Resizable with `[` / `]` in Detail focus.
    pub sidebar_width: u16,
    /// When `true`, the sidebar is hidden and the right pane uses the full width.
    /// Toggled by `\`.
    pub sidebar_hidden: bool,
    /// Scroll offset for the sidebar files list (not the right pane).
    pub pr_detail_sidebar_scroll: u16,
    /// `true` when the user pressed `g` in detail focus and is awaiting a second `g`.
    pub detail_pending_g: bool,
    /// Index into `pr_detail.commits` of the commit the user has scoped to.
    ///
    /// `None` = no scope = cumulative HEAD view (today's default behaviour).
    /// `Some(i)` = show only the delta introduced by `pr_detail.commits[i]`.
    pub selected_commit: Option<usize>,
    /// Per-commit diff fetches currently in flight.
    ///
    /// Keyed by `(repo_slug, full_sha)`. This dedupes eager prefetches kicked
    /// by PR-detail loads against on-demand fetches from pressing `Enter` in
    /// the Commits section.
    pub commit_diff_fetching: HashSet<(String, String)>,
    /// Cursor position in the Commits list (for `j`/`k` nav and the visual `▶`).
    ///
    /// Reset to 0 whenever a fresh `PrDetail` arrives.
    pub commits_cursor: usize,
    /// Which `(file_path, new_lineno)` thread-card anchors are currently
    /// expanded (showing full thread body rather than the collapsed summary).
    /// Ephemeral — not persisted across sessions. Cleared in `clear_detail_state`.
    pub pr_detail_expanded_threads: HashSet<(String, u32)>,
    /// The thread anchor `(file_path, new_lineno)` that the diff cursor is
    /// currently on or just past. Written each render frame by the
    /// `render_diff_with_threads` renderer; read by the `t` key handler to
    /// know which thread to toggle. `RefCell` because the renderer holds only
    /// `&App` but must write this field.
    pub pr_detail_diff_cursor: RefCell<Option<(String, u32)>>,
    /// Copy-mode state for the PR/issue detail view. Inactive until the user
    /// presses `v`. When active, normal detail key bindings are suppressed in
    /// favour of cursor movement, selection, and yank.
    pub copy_mode: crate::ui::copy_mode::CopyMode,
    /// Cached PR-detail right-pane viewport rect, written by the renderer each
    /// frame so copy-mode and mouse handlers can map screen coordinates to
    /// content positions.  Interior mutability via `Cell` keeps `&App` render
    /// signatures intact.
    pub pr_detail_viewport: std::cell::Cell<ratatui::layout::Rect>,
    /// Alias for `pr_detail_viewport` (right-pane inner rect).
    pub pr_detail_right_viewport: std::cell::Cell<ratatui::layout::Rect>,
    /// Cached sidebar rects `(sections_rect, files_rect)` for mouse hit-testing.
    pub pr_detail_sidebar_rects: std::cell::Cell<(ratatui::layout::Rect, ratatui::layout::Rect)>,

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

    // ── First-run wizard state ──────���─────────────────────────────────────────
    /// Suggested repos shown in the first-run wizard; populated from the inbox
    /// when `config.repos` is empty on the first successful fetch.
    pub first_run_suggestions: Vec<FirstRunSuggestion>,
    /// Index of the currently highlighted row in the first-run suggestion list.
    pub first_run_cursor: usize,

    // ── Theme picker state ────────────────────────────────────���───────────────
    /// Index of the currently highlighted theme in the picker list.
    pub theme_picker_cursor: usize,
    /// Focus state to restore when the theme picker is closed (Enter or Esc).
    pub theme_picker_return_focus: Focus,
    /// Theme that was active when the picker was opened; used by `Esc` to revert.
    pub theme_picker_original: crate::theme::Theme,
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

        // Snapshot the sidebar persistence fields before `session` is moved
        // into `Self`; the clamp mirrors the key handler so a malformed or
        // hand-edited state file can't produce a 3-column or 500-column bar.
        let sidebar_width_from_session = session.sidebar_width.clamp(20, 60);
        let sidebar_hidden_from_session = session.sidebar_hidden;

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
            per_tab_state: HashMap::new(),
            inbox_loaded_at: None,
            pending_g: false,
            flash: None,
            pr_detail: None,
            issue_detail: None,
            detail_fetching: false,
            detail_error: None,
            detail_cache: github::DetailCache::new(),
            detail_refreshing: None,
            pr_detail_scroll: HashMap::new(),
            pr_detail_diff_scroll: HashMap::new(),
            pr_detail_files_expanded: false,
            detail_comments_expanded: false,
            detail_show_outdated: true,
            thread_index: None,
            pr_detail_selected_section: DetailSection::default(),
            pr_detail_files_cursor: 0,
            pr_detail_files_show_diff: false,
            // Sidebar width / hidden restored from the persisted session so
            // `[`/`]` tweaks and the `\` toggle survive a relaunch. Clamp
            // the width here as a defensive measure against hand-edited
            // state files; the key handler clamps live edits separately.
            sidebar_width: sidebar_width_from_session,
            sidebar_hidden: sidebar_hidden_from_session,
            pr_detail_sidebar_scroll: 0,
            detail_pending_g: false,
            selected_commit: None,
            commit_diff_fetching: HashSet::new(),
            commits_cursor: 0,
            pr_detail_expanded_threads: HashSet::new(),
            pr_detail_diff_cursor: RefCell::new(None),
            copy_mode: crate::ui::copy_mode::CopyMode::default(),
            pr_detail_viewport: std::cell::Cell::new(ratatui::layout::Rect::default()),
            pr_detail_right_viewport: std::cell::Cell::new(ratatui::layout::Rect::default()),
            pr_detail_sidebar_rects: std::cell::Cell::new((
                ratatui::layout::Rect::default(),
                ratatui::layout::Rect::default(),
            )),
            repo_picker_list_cursor: 0,
            repo_picker_input: String::new(),
            repo_picker_mode: RepoPickerMode::List,
            repo_picker_return_focus: Focus::Dashboard,
            confirm: None,
            confirm_return_focus: Focus::Dashboard,
            first_run_suggestions: Vec::new(),
            first_run_cursor: 0,
            theme_picker_cursor: 0,
            theme_picker_return_focus: Focus::Dashboard,
            theme_picker_original: crate::theme::Theme::default(),
        }
    }

    /// Return the scroll offset for the given section (0 if never set).
    pub fn scroll_for(&self, section: DetailSection) -> u16 {
        self.pr_detail_scroll.get(&section).copied().unwrap_or(0)
    }

    /// Return a mutable reference to the scroll offset for the given section,
    /// inserting 0 if the section has not been scrolled yet.
    pub fn scroll_mut(&mut self, section: DetailSection) -> &mut u16 {
        self.pr_detail_scroll.entry(section).or_insert(0)
    }

    /// Path of the file currently highlighted in the Files sidebar, if that
    /// section is active and the PR has any files. Used to route right-pane
    /// scrolling to a per-file map when the Files section is showing a diff.
    pub(super) fn active_diff_file_path(&self) -> Option<String> {
        if self.pr_detail_selected_section != DetailSection::Files {
            return None;
        }
        let detail = self.pr_detail.as_ref()?;
        if detail.files.is_empty() {
            return None;
        }
        let idx = self.pr_detail_files_cursor.min(detail.files.len() - 1);
        Some(detail.files[idx].path.clone())
    }

    /// Current scroll offset for the right pane — dispatches to the per-file
    /// diff scroll map when viewing the Files section, otherwise to the
    /// per-section map. Keeps `j`/`k` natural no matter which section is
    /// active, and preserves scroll per-file when `J`/`K` cycles files.
    pub fn right_pane_scroll(&self) -> u16 {
        if let Some(path) = self.active_diff_file_path() {
            self.pr_detail_diff_scroll.get(&path).copied().unwrap_or(0)
        } else {
            self.scroll_for(self.pr_detail_selected_section)
        }
    }

    /// Mutable counterpart to [`Self::right_pane_scroll`], inserting a 0
    /// entry on first access so `*... += N` style updates compile.
    pub fn right_pane_scroll_mut(&mut self) -> &mut u16 {
        if let Some(path) = self.active_diff_file_path() {
            self.pr_detail_diff_scroll.entry(path).or_insert(0)
        } else {
            let section = self.pr_detail_selected_section;
            self.pr_detail_scroll.entry(section).or_insert(0)
        }
    }

    /// Display a flash message in the status bar for `duration`.
    ///
    /// Replaces any currently active flash message.
    pub fn show_flash(&mut self, text: impl Into<String>, duration: std::time::Duration) {
        self.flash = Some(crate::ui::status_bar::FlashMessage::new(text, duration));
    }
}
