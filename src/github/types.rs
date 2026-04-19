//! Domain types for the GitHub inbox data layer.
//!
//! These types represent the *normalised* view of GitHub data that the UI
//! consumes.  Raw GraphQL response shapes live in [`super::query`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Role ─────────────────────────────────────────────────────────────────────

/// How the viewer is related to a pull request.
///
/// A PR can appear in multiple search buckets; `roles` on [`PullRequest`] is
/// the union of all roles the viewer holds for that PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The viewer opened the PR.
    Author,
    /// The viewer was explicitly requested to review the PR.
    Reviewer,
    /// The viewer is assigned to the PR.
    Assignee,
}

// ── Mergeable ─────────────────────────────────────────────────────────────────

/// Whether a PR can be merged without conflicts.
///
/// Maps from GitHub's `mergeable` enum (`MERGEABLE`, `CONFLICTING`, `UNKNOWN`).
// The `Mergeable` variant intentionally shares the enum name — this mirrors
// the GitHub API field value exactly and is more readable than an alias.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Mergeable {
    Mergeable,
    Conflicting,
    Unknown,
}

// ── MergeStateStatus ─────────────────────────────────────────────────────────

/// Fine-grained merge-readiness state returned by the `mergeStateStatus` field.
///
/// The `#[serde(other)]` on `Unknown` catches any future variants GitHub adds
/// without breaking deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MergeStateStatus {
    /// All checks pass and the branch is up to date.
    Clean,
    /// The branch has merge conflicts.
    Dirty,
    /// At least one required check failed or a required review is missing.
    Blocked,
    /// The branch is behind the base branch.
    Behind,
    /// Non-required checks are failing.
    Unstable,
    /// The merge is gated by merge hooks.
    HasHooks,
    /// The PR is a draft.
    Draft,
    /// Unknown or future value.
    #[serde(other)]
    Unknown,
}

// ── ReviewDecision ────────────────────────────────────────────────────────────

/// Aggregate review decision for the PR.
///
/// Stored as `Option<ReviewDecision>` on [`PullRequest`]; `None` means no
/// decision has been recorded yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

// ── CheckState ────────────────────────────────────────────────────────────────

/// Rollup state of all commit status checks.
///
/// Maps from `statusCheckRollup.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckState {
    Success,
    Failure,
    Error,
    Pending,
    Expected,
}

// ── ReviewState ───────────────────────────────────────────────────────────────

/// State of an individual review submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
}

// ── Review ────────────────────────────────────────────────────────────────────

/// A single review left on a pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    /// GitHub login of the reviewer.
    pub author: String,
    /// Current state of the review.
    pub state: ReviewState,
}

// ── CheckRun ─────────────────────────────────────────────────────────────────

/// A single CI check run associated with the latest commit.
///
/// Only check runs with a `failure` or `error` conclusion appear in
/// [`PullRequest::failing_checks`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRun {
    /// Name of the individual job/check.
    pub name: String,
    /// Name of the parent workflow, if available.
    pub workflow_name: Option<String>,
    /// Final conclusion (`"failure"`, `"success"`, etc.).
    pub conclusion: Option<String>,
    /// Current status (`"completed"`, `"in_progress"`, etc.).
    pub status: String,
}

// ── PullRequest ───────────────────────────────────────────────────────────────

/// A normalised pull request from the viewer's inbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    /// PR number within the repository.
    pub number: u32,
    /// PR title.
    pub title: String,
    /// HTML URL.
    pub url: String,
    /// `owner/name` of the repository.
    pub repo: String,
    /// Login of the PR author.
    pub author: String,
    /// `true` when the PR is in draft state.
    pub is_draft: bool,
    /// Whether the branch can be merged without conflicts.
    pub mergeable: Mergeable,
    /// Fine-grained merge readiness.
    pub merge_state: MergeStateStatus,
    /// Aggregate review decision, if any.
    pub review_decision: Option<ReviewDecision>,
    /// Total number of commits on the PR.
    pub commits_count: u32,
    /// Total number of comments (not reviews).
    pub comments_count: u32,
    /// Rollup CI check state for the head commit.
    pub check_state: Option<CheckState>,
    /// CI check runs that have a `failure` or `error` conclusion.
    pub failing_checks: Vec<CheckRun>,
    /// Number of review threads that are neither resolved nor outdated.
    pub unresolved_threads: u32,
    /// Logins (or team names) of requested reviewers.
    pub requested_reviewers: Vec<String>,
    /// Latest reviews, one per reviewer.
    pub reviews: Vec<Review>,
    /// When the PR was last updated.
    pub updated_at: DateTime<Utc>,
    /// Roles the viewer holds on this PR (deduplicated union across search buckets).
    pub roles: Vec<Role>,
    /// Base branch name (e.g. `"main"`).
    ///
    /// Populated from the inbox query fragment so that the branch checkout flow
    /// (`c` key) can work from the dashboard without a separate detail fetch.
    pub base_ref: Option<String>,
    /// Head branch name (e.g. `"feat/my-feature"`).
    ///
    /// Same as `base_ref` — present at list level to enable `c` from the dashboard.
    pub head_ref: Option<String>,
}

// ── Label / Issue ─────────────────────────────────────────────────────────────

/// A GitHub label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    /// Label display name.
    pub name: String,
    /// Six-character hex colour string (without `#`).
    pub color: String,
}

/// A normalised GitHub issue from the viewer's inbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    /// Issue number within the repository.
    pub number: u32,
    /// Issue title.
    pub title: String,
    /// HTML URL.
    pub url: String,
    /// `owner/name` of the repository.
    pub repo: String,
    /// Login of the issue author.
    pub author: String,
    /// Total number of comments.
    pub comments_count: u32,
    /// When the issue was last updated.
    pub updated_at: DateTime<Utc>,
    /// Labels attached to the issue.
    pub labels: Vec<Label>,
}

// ── Inbox ─────────────────────────────────────────────────────────────────────

/// The viewer's complete GitHub inbox: pull requests and issues.
///
/// PRs are deduplicated across the author / reviewer / assignee search buckets;
/// each PR's `roles` field contains the union of all roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inbox {
    /// GitHub login of the authenticated viewer.
    pub viewer_login: String,
    /// Deduplicated list of open pull requests.
    pub prs: Vec<PullRequest>,
    /// Open issues assigned to the viewer.
    pub issues: Vec<Issue>,
}

// ── Display-order helpers ─────────────────────────────────────────────────────
//
// The dashboard table, the `Enter` → open-detail path, and the dashboard
// `o` / `y` URL resolvers all project the inbox down to "PRs (or issues) for
// one repo" and then index by a stored selection. If those three paths sort
// the slice differently, the selected row and the opened item drift apart —
// row N looks like PR X but click opens PR Y. Keep the sort in one place.

/// Filter the inbox's PRs to a single repo and sort for display.
///
/// Sort key: most-recently-updated first, with the PR number as a
/// deterministic tiebreaker so the order never flickers across refreshes
/// when two PRs share an `updated_at`.
#[must_use]
pub(crate) fn sorted_prs_for_repo<'a>(inbox: &'a Inbox, repo: &str) -> Vec<&'a PullRequest> {
    let mut prs: Vec<&PullRequest> = inbox.prs.iter().filter(|pr| pr.repo == repo).collect();
    prs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.number.cmp(&b.number)));
    prs
}

/// Filter the inbox's issues to a single repo and sort for display.
///
/// Same sort key as [`sorted_prs_for_repo`] so PRs and issues line up under
/// identical selection semantics.
#[must_use]
pub(crate) fn sorted_issues_for_repo<'a>(inbox: &'a Inbox, repo: &str) -> Vec<&'a Issue> {
    let mut issues: Vec<&Issue> = inbox.issues.iter().filter(|i| i.repo == repo).collect();
    issues.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.number.cmp(&b.number)));
    issues
}
