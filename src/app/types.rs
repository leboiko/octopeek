//! Plain data types shared across the `app` module.
//!
//! Contains enums and structs with no dependency on [`super::state::App`].
//! They live here so the heavier modules can import them without pulling in
//! the full state machinery.

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
    /// The theme picker overlay.
    ThemePicker,
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
/// Passed to [`super::state::App::spawn_detail_fetch`] so a single generic supervisor task
/// can dispatch to either [`crate::github::Client::fetch_pr_detail`] or
/// [`crate::github::Client::fetch_issue_detail`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailKind {
    /// Fetch a pull request.
    Pr,
    /// Fetch an issue.
    Issue,
}

/// Identifies the PR or issue the user had open in a given repo tab.
///
/// Persisted in [`crate::app::App::per_tab_state`] so that switching away from a tab and
/// returning to it restores the detail view the user was reading rather than
/// dumping them back on the dashboard list.
#[derive(Debug, Clone)]
pub struct DetailRef {
    pub repo: String,
    pub number: u32,
    pub kind: DetailKind,
}

/// State snapshot for a single repo tab, captured when the user switches away
/// from that tab and replayed when they come back.
///
/// Only the *reference* (repo + number + kind) is stored here. The actual
/// payload lives in [`crate::app::App::detail_cache`] and is served with
/// stale-while-revalidate semantics on restore: a cache hit shows content
/// immediately while a background re-fetch runs when the entry is stale.
/// A cold miss (first visit or after manual `r` refresh) shows the existing
/// "Fetching…" spinner as before.
#[derive(Debug, Clone, Default)]
pub struct PerTabState {
    /// PR or issue the user had open. `None` when they were on the list view.
    pub detail_ref: Option<DetailRef>,
}
