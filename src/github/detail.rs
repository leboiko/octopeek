//! Detail types and GraphQL queries for on-demand PR and issue fetching.
//!
//! List-level data (inbox) is handled by [`super::query`]. This module covers
//! the richer, per-item detail fetch that is triggered when the user opens a PR
//! or issue in the TUI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── FileChangeKind ────────────────────────────────────────────────────────────

/// The kind of change applied to a file in a pull request.
///
/// Maps from GitHub's GraphQL `PatchStatus` enum whose values are
/// `SCREAMING_SNAKE_CASE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    /// `CHANGED` is GitHub's catch-all for submodule updates etc.
    Changed,
}

// ── FileChange ────────────────────────────────────────────────────────────────

/// A single file touched by a pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// Repository-relative file path.
    pub path: String,
    /// Lines added in this file.
    pub additions: u32,
    /// Lines removed in this file.
    pub deletions: u32,
    /// Nature of the change.
    pub change_kind: FileChangeKind,
    /// Unified-diff patch text from the REST `pulls/{number}/files` endpoint.
    ///
    /// `None` when the file is binary, when the diff is too large for GitHub to
    /// return inline, or when the supplementary REST fetch was skipped / failed.
    #[serde(default)]
    pub patch: Option<String>,
}

// ── DetailedCheck ─────────────────────────────────────────────────────────────

/// A CI check run attached to the head commit of a pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedCheck {
    /// Name of the individual job/check step.
    pub name: String,
    /// Name of the parent GitHub Actions workflow, if available.
    pub workflow_name: Option<String>,
    /// `CheckStatusState` value (e.g. `"COMPLETED"`, `"IN_PROGRESS"`).
    pub status: String,
    /// `CheckConclusionState` value (e.g. `"SUCCESS"`, `"FAILURE"`), absent
    /// while the run is still in progress.
    pub conclusion: Option<String>,
    /// Wall-clock duration in seconds, derived from `completedAt - startedAt`.
    /// `None` if either timestamp is absent.
    pub duration_seconds: Option<u64>,
    /// URL to the detailed check-run page (if provided by GitHub).
    pub details_url: Option<String>,
}

// ── DetailedReview ────────────────────────────────────────────────────────────

/// A full review body, extending the list-level [`super::types::Review`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedReview {
    /// GitHub login of the reviewer.
    pub author: String,
    /// State of the review at submission time.
    pub state: crate::github::types::ReviewState,
    /// Markdown body of the review (may be empty for approve-only reviews).
    pub body_markdown: String,
    /// When the review was submitted.
    pub submitted_at: DateTime<Utc>,
}

// ── ReviewThread / ReviewComment ──────────────────────────────────────────────

/// A single inline comment on a specific line or hunk of a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    /// GitHub login of the comment author.
    pub author: String,
    /// Markdown body of the comment.
    pub body_markdown: String,
    /// When the comment was posted.
    pub created_at: DateTime<Utc>,
}

/// A thread of inline review comments anchored to a file location.
///
/// A thread may contain multiple replies (`comments`). The `path` and `line`
/// fields anchor it to a specific diff location; `line` is absent for
/// file-level (non-line) threads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThread {
    /// File path the thread is anchored to.
    pub path: String,
    /// Line number in the diff, if the thread is line-anchored.
    pub line: Option<u32>,
    /// `true` when all participants resolved the thread.
    pub is_resolved: bool,
    /// `true` when the thread's diff hunk no longer exists in the current diff.
    pub is_outdated: bool,
    /// Comments within this thread, in chronological order.
    pub comments: Vec<ReviewComment>,
}

// ── IssueComment ──────────────────────────────────────────────────────────────

/// A top-level comment on a pull request or issue (not an inline review comment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueComment {
    /// GitHub login of the comment author.
    pub author: String,
    /// Markdown body of the comment.
    pub body_markdown: String,
    /// When the comment was posted.
    pub created_at: DateTime<Utc>,
}

// ── PrDetail ─────────────────────────────────────────────────────────────────

/// Full detail for a single pull request, fetched on-demand.
///
/// The list-level [`super::types::PullRequest`] carries only fields needed for
/// the dashboard. This type adds bodies, files, checks, reviews, and threads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrDetail {
    /// `owner/name` repository slug.
    pub repo: String,
    /// PR number within the repository.
    pub number: u32,
    /// PR title.
    pub title: String,
    /// HTML URL.
    pub url: String,
    /// Login of the PR author.
    pub author: String,
    /// Raw Markdown body of the PR description.
    pub body_markdown: String,
    /// Base branch name (e.g. `"main"`).
    pub base_ref: String,
    /// Head branch name (e.g. `"feat/xyz"`).
    pub head_ref: String,
    /// `true` when the PR is in draft state.
    pub is_draft: bool,
    /// Total lines added across all files.
    pub additions: u32,
    /// Total lines removed across all files.
    pub deletions: u32,
    /// Number of files changed.
    pub changed_files_count: u32,
    /// When the PR was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the PR was created.
    pub created_at: DateTime<Utc>,
    /// `true` when the PR has been merged.
    pub merged: bool,
    /// Files changed by this PR (up to 100).
    pub files: Vec<FileChange>,
    /// Check runs on the head commit (up to 50).
    pub check_runs: Vec<DetailedCheck>,
    /// Reviews left on this PR (up to 50).
    pub reviews: Vec<DetailedReview>,
    /// Inline review threads (up to 100), each with up to 20 comments.
    pub review_threads: Vec<ReviewThread>,
    /// Top-level PR comments (up to 100).
    pub issue_comments: Vec<IssueComment>,
}

// ── IssueDetail ───────────────────────────────────────────────────────────────

/// Full detail for a single issue, fetched on-demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueDetail {
    /// `owner/name` repository slug.
    pub repo: String,
    /// Issue number within the repository.
    pub number: u32,
    /// Issue title.
    pub title: String,
    /// HTML URL.
    pub url: String,
    /// Login of the issue author.
    pub author: String,
    /// Raw Markdown body of the issue.
    pub body_markdown: String,
    /// State of the issue (`"OPEN"` or `"CLOSED"`).
    pub state: String,
    /// When the issue was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the issue was created.
    pub created_at: DateTime<Utc>,
    /// Labels attached to the issue.
    pub labels: Vec<crate::github::types::Label>,
    /// Logins of users assigned to this issue.
    pub assignees: Vec<String>,
    /// Top-level comments on the issue (up to 100).
    pub comments: Vec<IssueComment>,
}

// ── GraphQL query strings ─────────────────────────────────────────────────────

/// GraphQL document for fetching full PR detail.
///
/// Parameters: `$owner: String!`, `$name: String!`, `$number: Int!`
pub(super) const PR_DETAIL_QUERY: &str = r"
query PrDetail($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      number
      title
      url
      isDraft
      merged
      body
      createdAt
      updatedAt
      additions
      deletions
      changedFiles
      baseRefName
      headRefName
      author { login }
      files(first: 100) {
        nodes {
          path
          additions
          deletions
          changeType
        }
      }
      commits(last: 1) {
        nodes {
          commit {
            statusCheckRollup {
              contexts(first: 50) {
                nodes {
                  ... on CheckRun {
                    name
                    status
                    conclusion
                    startedAt
                    completedAt
                    detailsUrl
                    checkSuite {
                      workflowRun {
                        workflow { name }
                      }
                    }
                  }
                  ... on StatusContext {
                    context
                    state
                    targetUrl
                  }
                }
              }
            }
          }
        }
      }
      reviews(first: 50) {
        nodes {
          author { login }
          state
          body
          submittedAt
        }
      }
      reviewThreads(first: 100) {
        nodes {
          isResolved
          isOutdated
          path
          line
          originalLine
          comments(first: 20) {
            nodes {
              author { login }
              body
              createdAt
            }
          }
        }
      }
      comments(first: 100) {
        nodes {
          author { login }
          body
          createdAt
        }
      }
    }
  }
}
";

/// GraphQL document for fetching full issue detail.
///
/// Parameters: `$owner: String!`, `$name: String!`, `$number: Int!`
pub(super) const ISSUE_DETAIL_QUERY: &str = r"
query IssueDetail($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    issue(number: $number) {
      number
      title
      url
      body
      state
      createdAt
      updatedAt
      author { login }
      labels(first: 30) {
        nodes {
          name
          color
        }
      }
      assignees(first: 20) {
        nodes { login }
      }
      comments(first: 100) {
        nodes {
          author { login }
          body
          createdAt
        }
      }
    }
  }
}
";

// ── Raw deserialization types ─────────────────────────────────────────────────
//
// These mirror the GraphQL response shape exactly.  They are private; callers
// always receive the public domain structs above. The top-level envelope
// (`data` / `errors`) is the generic `GqlEnvelope<RawDetailData>` defined in
// `super::query` so the HTTP client can share one helper across every query.

#[derive(Debug, Deserialize)]
pub(super) struct RawDetailData {
    pub repository: Option<RawDetailRepository>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawDetailRepository {
    pub pull_request: Option<RawPrDetail>,
    pub issue: Option<RawIssueDetail>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawPrDetail {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub is_draft: bool,
    pub merged: bool,
    pub body: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub additions: u32,
    pub deletions: u32,
    pub changed_files: u32,
    pub base_ref_name: String,
    pub head_ref_name: String,
    pub author: Option<RawDetailActor>,
    pub files: RawNodeList<RawFileNode>,
    pub commits: RawNodeList<RawDetailCommitNode>,
    pub reviews: RawNodeList<RawReviewNode>,
    pub review_threads: RawNodeList<RawReviewThreadNode>,
    pub comments: RawNodeList<RawCommentNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawIssueDetail {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub body: Option<String>,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub author: Option<RawDetailActor>,
    pub labels: RawNodeList<RawLabelNode>,
    pub assignees: RawNodeList<RawDetailActor>,
    pub comments: RawNodeList<RawCommentNode>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawDetailActor {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawNodeList<T> {
    pub nodes: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawFileNode {
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
    pub change_type: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawDetailCommitNode {
    pub commit: RawDetailCommit,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawDetailCommit {
    pub status_check_rollup: Option<RawDetailRollup>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawDetailRollup {
    pub contexts: RawNodeList<RawDetailCheckContext>,
}

/// Inline-fragment union: either a `CheckRun` or a `StatusContext`.
///
/// `serde`'s `untagged` enum tries each variant in order; `RawDetailCheckRun`
/// is tried first because it has a `name` field that `RawDetailStatusContext`
/// does not (it uses `context`).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum RawDetailCheckContext {
    CheckRun(RawDetailCheckRun),
    StatusContext(RawDetailStatusContext),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawDetailCheckRun {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub details_url: Option<String>,
    pub check_suite: Option<RawDetailCheckSuite>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawDetailStatusContext {
    pub context: String,
    pub state: String,
    pub target_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawDetailCheckSuite {
    pub workflow_run: Option<RawDetailWorkflowRun>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawDetailWorkflowRun {
    pub workflow: Option<RawDetailWorkflow>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawDetailWorkflow {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawReviewNode {
    pub author: Option<RawDetailActor>,
    pub state: crate::github::types::ReviewState,
    pub body: Option<String>,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawReviewThreadNode {
    pub is_resolved: bool,
    pub is_outdated: bool,
    pub path: String,
    /// Preferred line field; falls back to `original_line` when absent.
    pub line: Option<u32>,
    pub original_line: Option<u32>,
    pub comments: RawNodeList<RawCommentNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawCommentNode {
    pub author: Option<RawDetailActor>,
    pub body: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Convert a raw GraphQL comment node into the public [`IssueComment`] type.
///
/// The same shape appears on a PR (`raw_pr_to_detail`) and on an issue
/// (`raw_issue_to_detail`); this helper keeps the deleted-author sentinel
/// and the `body.unwrap_or_default()` handling in one place.
fn map_comment_node(c: RawCommentNode) -> IssueComment {
    IssueComment {
        author: crate::github::author_or_deleted(c.author.map(|a| a.login)),
        body_markdown: c.body.unwrap_or_default(),
        created_at: c.created_at,
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct RawLabelNode {
    pub name: String,
    pub color: String,
}

// ── Domain conversion helpers ─────────────────────────────────────────────────

/// Parse a `changeType` `SCREAMING_SNAKE_CASE` string into [`FileChangeKind`].
///
/// Unknown values fall back to [`FileChangeKind::Changed`] to avoid panics on
/// future GitHub API additions.
fn parse_change_kind(s: &str) -> FileChangeKind {
    match s {
        "ADDED" => FileChangeKind::Added,
        "MODIFIED" => FileChangeKind::Modified,
        "DELETED" => FileChangeKind::Deleted,
        "RENAMED" => FileChangeKind::Renamed,
        "COPIED" => FileChangeKind::Copied,
        // CHANGED and any future unknown values.
        _ => FileChangeKind::Changed,
    }
}

/// Convert a [`RawPrDetail`] to the public [`PrDetail`] domain type.
///
/// The `repo` slug is threaded in from the caller (it is not present in the
/// PR-level GraphQL fragment).
#[allow(clippy::too_many_lines)]
pub(super) fn raw_pr_to_detail(repo: String, raw: RawPrDetail) -> PrDetail {
    let files = raw
        .files
        .nodes
        .into_iter()
        .map(|f| FileChange {
            path: f.path,
            additions: f.additions,
            deletions: f.deletions,
            change_kind: parse_change_kind(&f.change_type),
            patch: None,
        })
        .collect();

    // Dig into commits → last commit → statusCheckRollup → contexts.
    let check_runs = raw
        .commits
        .nodes
        .into_iter()
        .next()
        .and_then(|cn| cn.commit.status_check_rollup)
        .map(|rollup| {
            rollup
                .contexts
                .nodes
                .into_iter()
                .map(|ctx| match ctx {
                    RawDetailCheckContext::CheckRun(cr) => {
                        // Compute duration from the two nullable timestamps.
                        // Both must be `Some` and `completed >= started`.
                        let duration_seconds =
                            cr.started_at.zip(cr.completed_at).and_then(|(s, c)| {
                                let delta = c.signed_duration_since(s).num_seconds();
                                // Cast is safe: negative duration means clock skew;
                                // treat as None rather than wrapping to a huge value.
                                if delta >= 0 {
                                    #[allow(clippy::cast_sign_loss)]
                                    Some(delta as u64)
                                } else {
                                    None
                                }
                            });

                        let workflow_name = cr
                            .check_suite
                            .as_ref()
                            .and_then(|cs| cs.workflow_run.as_ref())
                            .and_then(|wr| wr.workflow.as_ref())
                            .map(|w| w.name.clone());

                        DetailedCheck {
                            name: cr.name,
                            workflow_name,
                            status: cr.status,
                            conclusion: cr.conclusion,
                            duration_seconds,
                            details_url: cr.details_url,
                        }
                    }
                    RawDetailCheckContext::StatusContext(sc) => DetailedCheck {
                        name: sc.context,
                        workflow_name: None,
                        status: "COMPLETED".to_owned(),
                        conclusion: Some(sc.state),
                        duration_seconds: None,
                        details_url: sc.target_url,
                    },
                })
                .collect()
        })
        .unwrap_or_default();

    let reviews = raw
        .reviews
        .nodes
        .into_iter()
        .filter_map(|r| {
            // Reviews without an author login are bot-generated or deleted
            // accounts — skip them rather than surfacing a blank name.
            r.author.map(|a| DetailedReview {
                author: a.login,
                state: r.state,
                body_markdown: r.body.unwrap_or_default(),
                submitted_at: r.submitted_at,
            })
        })
        .collect();

    // Each thread node carries a list of comment nodes. We flatten the nested
    // structure into `ReviewThread { comments: Vec<ReviewComment> }`.
    let review_threads = raw
        .review_threads
        .nodes
        .into_iter()
        .map(|t| {
            // `line` is the current-diff line; `original_line` is the pre-rebase
            // line. Prefer `line` (most useful for display); fall back to
            // `original_line` when the hunk has shifted.
            let line = t.line.or(t.original_line);

            let comments = t
                .comments
                .nodes
                .into_iter()
                .map(|c| ReviewComment {
                    author: crate::github::author_or_deleted(c.author.map(|a| a.login)),
                    body_markdown: c.body.unwrap_or_default(),
                    created_at: c.created_at,
                })
                .collect();

            ReviewThread {
                path: t.path,
                line,
                is_resolved: t.is_resolved,
                is_outdated: t.is_outdated,
                comments,
            }
        })
        .collect();

    let issue_comments = raw.comments.nodes.into_iter().map(map_comment_node).collect();

    PrDetail {
        repo,
        number: raw.number,
        title: raw.title,
        url: raw.url,
        author: crate::github::author_or_deleted(raw.author.map(|a| a.login)),
        body_markdown: raw.body.unwrap_or_default(),
        base_ref: raw.base_ref_name,
        head_ref: raw.head_ref_name,
        is_draft: raw.is_draft,
        additions: raw.additions,
        deletions: raw.deletions,
        changed_files_count: raw.changed_files,
        updated_at: raw.updated_at,
        created_at: raw.created_at,
        merged: raw.merged,
        files,
        check_runs,
        reviews,
        review_threads,
        issue_comments,
    }
}

/// Convert a [`RawIssueDetail`] to the public [`IssueDetail`] domain type.
pub(super) fn raw_issue_to_detail(repo: String, raw: RawIssueDetail) -> IssueDetail {
    let labels = raw
        .labels
        .nodes
        .into_iter()
        .map(|l| crate::github::types::Label { name: l.name, color: l.color })
        .collect();

    let assignees = raw.assignees.nodes.into_iter().map(|a| a.login).collect();

    let comments = raw.comments.nodes.into_iter().map(map_comment_node).collect();

    IssueDetail {
        repo,
        number: raw.number,
        title: raw.title,
        url: raw.url,
        author: crate::github::author_or_deleted(raw.author.map(|a| a.login)),
        body_markdown: raw.body.unwrap_or_default(),
        state: raw.state,
        updated_at: raw.updated_at,
        created_at: raw.created_at,
        labels,
        assignees,
        comments,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::github::query::GqlEnvelope;

    /// Type alias kept private to the test module so the existing test names
    /// stay unchanged. The envelope is now generic in non-test code.
    type RawDetailResponse = GqlEnvelope<RawDetailData>;

    // ── Fixture helpers ───────────────────────────────────────────────────────

    fn pr_detail_fixture() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "repository": {
                    "pullRequest": {
                        "number": 42,
                        "title": "feat: add dark mode",
                        "url": "https://github.com/owner/repo/pull/42",
                        "isDraft": false,
                        "merged": false,
                        "body": "## Summary\nAdds dark mode support.",
                        "createdAt": "2024-01-01T10:00:00Z",
                        "updatedAt": "2024-01-02T12:00:00Z",
                        "additions": 150,
                        "deletions": 30,
                        "changedFiles": 5,
                        "baseRefName": "main",
                        "headRefName": "feat/dark-mode",
                        "author": { "login": "alice" },
                        "files": {
                            "nodes": [
                                {
                                    "path": "src/theme.rs",
                                    "additions": 100,
                                    "deletions": 10,
                                    "changeType": "MODIFIED"
                                },
                                {
                                    "path": "src/new_file.rs",
                                    "additions": 50,
                                    "deletions": 0,
                                    "changeType": "ADDED"
                                }
                            ]
                        },
                        "commits": {
                            "nodes": [{
                                "commit": {
                                    "statusCheckRollup": {
                                        "contexts": {
                                            "nodes": [{
                                                "name": "ci / build",
                                                "status": "COMPLETED",
                                                "conclusion": "SUCCESS",
                                                "startedAt": "2024-01-02T11:00:00Z",
                                                "completedAt": "2024-01-02T11:05:00Z",
                                                "detailsUrl": "https://github.com/checks/1",
                                                "checkSuite": {
                                                    "workflowRun": {
                                                        "workflow": { "name": "CI" }
                                                    }
                                                }
                                            }]
                                        }
                                    }
                                }
                            }]
                        },
                        "reviews": {
                            "nodes": [{
                                "author": { "login": "bob" },
                                "state": "APPROVED",
                                "body": "LGTM!",
                                "submittedAt": "2024-01-02T09:00:00Z"
                            }]
                        },
                        "reviewThreads": {
                            "nodes": [{
                                "isResolved": false,
                                "isOutdated": false,
                                "path": "src/theme.rs",
                                "line": 42,
                                "originalLine": 40,
                                "comments": {
                                    "nodes": [
                                        {
                                            "author": { "login": "bob" },
                                            "body": "Consider extracting this constant.",
                                            "createdAt": "2024-01-02T09:05:00Z"
                                        },
                                        {
                                            "author": { "login": "alice" },
                                            "body": "Good point, will do.",
                                            "createdAt": "2024-01-02T09:10:00Z"
                                        }
                                    ]
                                }
                            }]
                        },
                        "comments": {
                            "nodes": [{
                                "author": { "login": "carol" },
                                "body": "Nice work!",
                                "createdAt": "2024-01-02T10:00:00Z"
                            }]
                        }
                    }
                }
            }
        })
    }

    fn issue_detail_fixture() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "repository": {
                    "issue": {
                        "number": 7,
                        "title": "Bug: crash on empty config",
                        "url": "https://github.com/owner/repo/issues/7",
                        "body": "Reproducible with an empty `config.toml`.",
                        "state": "OPEN",
                        "createdAt": "2024-01-01T08:00:00Z",
                        "updatedAt": "2024-01-01T09:00:00Z",
                        "author": { "login": "dave" },
                        "labels": {
                            "nodes": [
                                { "name": "bug", "color": "ee0701" }
                            ]
                        },
                        "assignees": {
                            "nodes": [{ "login": "alice" }]
                        },
                        "comments": {
                            "nodes": [{
                                "author": { "login": "bob" },
                                "body": "I can reproduce this too.",
                                "createdAt": "2024-01-01T08:30:00Z"
                            }]
                        }
                    }
                }
            }
        })
    }

    // ── Deserialization tests ─────────────────────────────────────────────────

    /// Full PR detail fixture must deserialise and convert to `PrDetail`.
    #[test]
    fn pr_detail_deserialises_correctly() {
        let json = pr_detail_fixture();
        let raw: RawDetailResponse = serde_json::from_value(json).expect("deserialise");
        let repo_raw = raw
            .data
            .expect("data")
            .repository
            .expect("repository")
            .pull_request
            .expect("pull_request");
        let detail = raw_pr_to_detail("owner/repo".to_owned(), repo_raw);

        assert_eq!(detail.number, 42);
        assert_eq!(detail.author, "alice");
        assert_eq!(detail.base_ref, "main");
        assert_eq!(detail.head_ref, "feat/dark-mode");
        assert_eq!(detail.additions, 150);
        assert_eq!(detail.changed_files_count, 5);
        assert!(!detail.merged);
    }

    /// Full issue detail fixture must deserialise and convert to `IssueDetail`.
    #[test]
    fn issue_detail_deserialises_correctly() {
        let json = issue_detail_fixture();
        let raw: RawDetailResponse = serde_json::from_value(json).expect("deserialise");
        let repo_raw =
            raw.data.expect("data").repository.expect("repository").issue.expect("issue");
        let detail = raw_issue_to_detail("owner/repo".to_owned(), repo_raw);

        assert_eq!(detail.number, 7);
        assert_eq!(detail.state, "OPEN");
        assert_eq!(detail.assignees, vec!["alice"]);
        assert_eq!(detail.labels.len(), 1);
        assert_eq!(detail.labels[0].name, "bug");
        assert_eq!(detail.comments.len(), 1);
    }

    // ── FileChangeKind mapping ────────────────────────────────────────────────

    /// All documented `changeType` values must map to the correct enum variant.
    #[test]
    fn file_change_kind_all_variants() {
        assert_eq!(parse_change_kind("ADDED"), FileChangeKind::Added);
        assert_eq!(parse_change_kind("MODIFIED"), FileChangeKind::Modified);
        assert_eq!(parse_change_kind("DELETED"), FileChangeKind::Deleted);
        assert_eq!(parse_change_kind("RENAMED"), FileChangeKind::Renamed);
        assert_eq!(parse_change_kind("COPIED"), FileChangeKind::Copied);
        assert_eq!(parse_change_kind("CHANGED"), FileChangeKind::Changed);
    }

    /// Unknown `changeType` values must fall back to `Changed` without panicking.
    #[test]
    fn file_change_kind_unknown_falls_back() {
        assert_eq!(parse_change_kind("FUTURE_VARIANT"), FileChangeKind::Changed);
    }

    // ── duration_seconds computation ──────────────────────────────────────────

    /// `duration_seconds` must be computed correctly from `startedAt` / `completedAt`.
    #[test]
    fn check_run_duration_computed() {
        let json = pr_detail_fixture();
        let raw: RawDetailResponse = serde_json::from_value(json).expect("deserialise");
        let repo_raw = raw
            .data
            .expect("data")
            .repository
            .expect("repository")
            .pull_request
            .expect("pull_request");
        let detail = raw_pr_to_detail("owner/repo".to_owned(), repo_raw);

        assert_eq!(detail.check_runs.len(), 1);
        // 11:05 − 11:00 = 300 seconds
        assert_eq!(detail.check_runs[0].duration_seconds, Some(300));
        assert_eq!(detail.check_runs[0].workflow_name.as_deref(), Some("CI"));
    }

    /// A check run with only `startedAt` set (still in progress) must have
    /// `duration_seconds == None`.
    #[test]
    fn check_run_duration_none_when_incomplete() {
        // Build a minimal PR JSON with only startedAt set.
        let json = serde_json::json!({
            "number": 1, "title": "t", "url": "u", "isDraft": false, "merged": false,
            "body": null, "createdAt": "2024-01-01T00:00:00Z",
            "updatedAt": "2024-01-01T00:00:00Z",
            "additions": 0, "deletions": 0, "changedFiles": 0,
            "baseRefName": "main", "headRefName": "feat",
            "author": null,
            "files": { "nodes": [] },
            "commits": { "nodes": [{
                "commit": { "statusCheckRollup": { "contexts": { "nodes": [{
                    "name": "build",
                    "status": "IN_PROGRESS",
                    "conclusion": null,
                    "startedAt": "2024-01-01T00:00:00Z",
                    "completedAt": null,
                    "detailsUrl": null,
                    "checkSuite": null
                }] } } }
            }] },
            "reviews": { "nodes": [] },
            "reviewThreads": { "nodes": [] },
            "comments": { "nodes": [] }
        });
        let raw: RawPrDetail = serde_json::from_value(json).expect("deserialise");
        let detail = raw_pr_to_detail("owner/repo".to_owned(), raw);
        assert_eq!(detail.check_runs[0].duration_seconds, None);
    }

    // ── ReviewThread multi-comment ────────────────────────────────────────────

    /// A thread with two comments must produce exactly two `ReviewComment` items
    /// in the correct order.
    #[test]
    fn review_thread_preserves_comment_order() {
        let json = pr_detail_fixture();
        let raw: RawDetailResponse = serde_json::from_value(json).expect("deserialise");
        let repo_raw = raw
            .data
            .expect("data")
            .repository
            .expect("repository")
            .pull_request
            .expect("pull_request");
        let detail = raw_pr_to_detail("owner/repo".to_owned(), repo_raw);

        assert_eq!(detail.review_threads.len(), 1);
        let thread = &detail.review_threads[0];
        assert_eq!(thread.comments.len(), 2);
        assert_eq!(thread.comments[0].author, "bob");
        assert_eq!(thread.comments[1].author, "alice");
        // `line` should prefer the `line` field over `originalLine`.
        assert_eq!(thread.line, Some(42));
    }
}
