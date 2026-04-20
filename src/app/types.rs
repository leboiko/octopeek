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
    /// The comment/reply composer overlay.
    Composer,
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Detail item kind that can receive a top-level comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentSubjectKind {
    /// Comment on a pull request conversation.
    PullRequest,
    /// Comment on an issue conversation.
    Issue,
}

/// Destination for the markdown composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentComposerTarget {
    /// Add a top-level issue-style comment to a PR or issue.
    TopLevel {
        /// Repository slug in `owner/name` form.
        repo: String,
        /// PR or issue number.
        number: u32,
        /// GraphQL node ID of the PR or issue.
        subject_id: String,
        /// The type of subject being commented on.
        kind: CommentSubjectKind,
    },
    /// Add a reply to an existing pull request review thread.
    ReviewThreadReply {
        /// Repository slug in `owner/name` form.
        repo: String,
        /// Pull request number.
        number: u32,
        /// GraphQL node ID of the review thread.
        thread_id: String,
        /// File path shown for user context.
        path: String,
        /// Optional line number shown for user context.
        line: Option<u32>,
    },
}

impl CommentComposerTarget {
    /// Short label shown in the composer title.
    pub fn label(&self) -> &'static str {
        match self {
            Self::TopLevel { kind: CommentSubjectKind::PullRequest, .. } => "PR comment",
            Self::TopLevel { kind: CommentSubjectKind::Issue, .. } => "Issue comment",
            Self::ReviewThreadReply { .. } => "Thread reply",
        }
    }

    /// Stable refresh target after a successful comment mutation.
    pub fn detail_ref(&self) -> DetailRef {
        match self {
            Self::TopLevel { repo, number, kind, .. } => DetailRef {
                repo: repo.clone(),
                number: *number,
                kind: match kind {
                    CommentSubjectKind::PullRequest => DetailKind::Pr,
                    CommentSubjectKind::Issue => DetailKind::Issue,
                },
            },
            Self::ReviewThreadReply { repo, number, .. } => {
                DetailRef { repo: repo.clone(), number: *number, kind: DetailKind::Pr }
            }
        }
    }
}

/// Full state for the markdown composer overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentComposer {
    /// Destination for the comment body.
    pub target: CommentComposerTarget,
    /// Markdown buffer being edited.
    pub body: String,
}

impl CommentComposer {
    /// Create an empty composer for `target`.
    pub fn new(target: CommentComposerTarget) -> Self {
        Self { target, body: String::new() }
    }
}

/// User-visible mutation currently in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingMutation {
    /// Merge a pull request.
    MergePullRequest {
        /// Repository slug in `owner/name` form.
        repo: String,
        /// Pull request number.
        number: u32,
        /// Selected merge method.
        method: crate::github::mutations::MergeMethod,
    },
    /// Add a top-level PR/issue comment or a review-thread reply.
    SubmitComment {
        /// Destination for the submitted comment.
        target: CommentComposerTarget,
    },
}

impl PendingMutation {
    /// Text shown in the status bar while the mutation is running.
    pub fn label(&self) -> String {
        match self {
            Self::MergePullRequest { repo, number, method } => {
                format!("{} {repo}#{number}", method.label())
            }
            Self::SubmitComment { target } => match target {
                CommentComposerTarget::TopLevel { repo, number, .. } => {
                    format!("comment {repo}#{number}")
                }
                CommentComposerTarget::ReviewThreadReply { repo, number, .. } => {
                    format!("reply {repo}#{number}")
                }
            },
        }
    }
}

/// Detail target to refresh after a successful GitHub mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationRefresh {
    /// Detail item that changed.
    pub detail_ref: DetailRef,
    /// Status-bar message for the success path.
    pub message: String,
}
