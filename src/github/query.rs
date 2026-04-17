//! GraphQL query string and response-to-domain conversion.
//!
//! Raw GraphQL response structs live here and are intentionally kept private;
//! callers receive [`super::types::Inbox`] from [`to_inbox`].

use std::collections::HashMap;

use serde::Deserialize;

use super::types::{
    CheckRun, CheckState, Inbox, Issue, Label, MergeStateStatus, Mergeable, PullRequest, Review,
    ReviewDecision, ReviewState, Role,
};

// ── Query string ──────────────────────────────────────────────────────────────

/// The single GraphQL document sent to `api.github.com/graphql`.
///
/// Four aliased top-level fields are merged by [`to_inbox`] into one [`Inbox`].
pub const INBOX_QUERY: &str = r#"
query InboxQuery {
  authored: viewer {
    login
    pullRequests(first: 50, states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes {
        ...PullRequestFields
      }
    }
  }
  reviewRequested: search(query: "is:open is:pr review-requested:@me", type: ISSUE, first: 50) {
    nodes {
      ... on PullRequest {
        ...PullRequestFields
      }
    }
  }
  assignedPrs: search(query: "is:open is:pr assignee:@me", type: ISSUE, first: 50) {
    nodes {
      ... on PullRequest {
        ...PullRequestFields
      }
    }
  }
  assignedIssues: search(query: "is:open is:issue assignee:@me", type: ISSUE, first: 50) {
    nodes {
      ... on Issue {
        ...IssueFields
      }
    }
  }
}

fragment PullRequestFields on PullRequest {
  number
  title
  url
  isDraft
  mergeable
  mergeStateStatus
  reviewDecision
  repository { nameWithOwner }
  author { login }
  updatedAt
  commits(last: 1) {
    totalCount
    nodes {
      commit {
        statusCheckRollup {
          state
          contexts(first: 20) {
            nodes {
              ... on CheckRun {
                name
                status
                conclusion
                checkSuite { workflowRun { workflow { name } } }
              }
              ... on StatusContext {
                context
                state
              }
            }
          }
        }
      }
    }
  }
  comments { totalCount }
  reviewRequests(first: 10) {
    nodes {
      requestedReviewer {
        ... on User { login }
        ... on Team { name }
      }
    }
  }
  reviewThreads(first: 30) {
    nodes {
      isResolved
      isOutdated
    }
  }
  latestReviews(first: 10) {
    nodes {
      author { login }
      state
    }
  }
}

fragment IssueFields on Issue {
  number
  title
  url
  repository { nameWithOwner }
  author { login }
  updatedAt
  comments { totalCount }
  labels(first: 20) {
    nodes {
      name
      color
    }
  }
}
"#;

// ── Raw GraphQL response types ────────────────────────────────────────────────

/// Top-level GraphQL response envelope.
#[derive(Debug, Deserialize)]
pub struct GraphQlResponse {
    pub data: Option<ResponseData>,
    pub errors: Option<Vec<GraphQlError>>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQlError {
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseData {
    /// `viewer { login, pullRequests { nodes } }` — authored PRs.
    pub authored: AuthoredViewer,
    /// `search(...)` for PRs with review-requested.
    pub review_requested: SearchResult,
    /// `search(...)` for PRs assigned to viewer.
    pub assigned_prs: SearchResult,
    /// `search(...)` for issues assigned to viewer.
    pub assigned_issues: SearchResult,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoredViewer {
    pub login: String,
    pub pull_requests: NodeList<RawPr>,
}

#[derive(Debug, Deserialize)]
pub struct SearchResult {
    pub nodes: Vec<Option<SearchNode>>,
}

/// A node from an inline fragment — may be a PR or something else (ignored).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SearchNode {
    Pr(RawPr),
    Issue(RawIssue),
}

#[derive(Debug, Deserialize)]
pub struct NodeList<T> {
    pub nodes: Vec<T>,
}

// ── Raw PR shape ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPr {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub is_draft: bool,
    pub mergeable: Mergeable,
    pub merge_state_status: MergeStateStatus,
    pub review_decision: Option<ReviewDecision>,
    pub repository: RawRepo,
    pub author: Option<RawActor>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub commits: RawCommits,
    pub comments: RawTotalCount,
    pub review_requests: NodeList<RawReviewRequest>,
    pub review_threads: NodeList<RawReviewThread>,
    pub latest_reviews: NodeList<RawReview>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCommits {
    pub total_count: u32,
    pub nodes: Vec<RawCommitNode>,
}

#[derive(Debug, Deserialize)]
pub struct RawCommitNode {
    pub commit: RawCommit,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCommit {
    pub status_check_rollup: Option<RawStatusRollup>,
}

#[derive(Debug, Deserialize)]
pub struct RawStatusRollup {
    pub state: CheckState,
    pub contexts: NodeList<RawCheckContext>,
}

/// Inline-fragment union: either a `CheckRun` or a `StatusContext`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawCheckContext {
    CheckRun(RawCheckRun),
    /// Commit-status context — deserialized for the untagged enum discriminator;
    /// the inner data is intentionally unused (only `CheckRun` entries are surfaced).
    #[allow(dead_code)]
    StatusContext(RawStatusContext),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCheckRun {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    /// Nested path: `checkSuite.workflowRun.workflow.name`
    pub check_suite: Option<RawCheckSuite>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCheckSuite {
    pub workflow_run: Option<RawWorkflowRun>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWorkflowRun {
    pub workflow: Option<RawWorkflow>,
}

#[derive(Debug, Deserialize)]
pub struct RawWorkflow {
    pub name: String,
}

/// A legacy commit-status context (not a GitHub Actions check run).
///
/// The fields are present in the JSON but we do not surface them in the
/// domain layer — only `CheckRun` contexts contribute to `failing_checks`.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct RawStatusContext {
    pub context: String,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct RawTotalCount {
    #[serde(rename = "totalCount")]
    pub total_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawReviewRequest {
    pub requested_reviewer: Option<RawReviewer>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawReviewer {
    User { login: String },
    Team { name: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawReviewThread {
    pub is_resolved: bool,
    pub is_outdated: bool,
}

#[derive(Debug, Deserialize)]
pub struct RawReview {
    pub author: Option<RawActor>,
    pub state: ReviewState,
}

#[derive(Debug, Deserialize)]
pub struct RawRepo {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: String,
}

#[derive(Debug, Deserialize)]
pub struct RawActor {
    pub login: String,
}

// ── Raw Issue shape ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawIssue {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub repository: RawRepo,
    pub author: Option<RawActor>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub comments: RawTotalCount,
    pub labels: NodeList<RawLabel>,
}

#[derive(Debug, Deserialize)]
pub struct RawLabel {
    pub name: String,
    pub color: String,
}

// ── Conversion: raw → domain ───────────────────────────────────────────────────

/// Convert a raw GraphQL [`ResponseData`] into a normalised [`Inbox`].
///
/// # Deduplication
///
/// PRs that appear in more than one search bucket (e.g. the viewer authored a
/// PR and is also a requested reviewer) are merged: the `roles` field becomes
/// the union of all roles for that `(repo, number)` pair.
///
/// # Errors
///
/// Returns any errors embedded in the response; if both `data` and `errors`
/// are present, converts `data` and logs errors via the caller.
pub fn to_inbox(viewer_login: String, data: ResponseData) -> Inbox {
    // Key: (repo nameWithOwner, pr number)
    type PrKey = (String, u32);
    let mut pr_map: HashMap<PrKey, (PullRequest, Vec<Role>)> = HashMap::new();

    // Helper closure: insert or merge a raw PR with a given role.
    let mut upsert = |raw: RawPr, role: Role| {
        let key = (raw.repository.name_with_owner.clone(), raw.number);
        let entry = pr_map.entry(key);
        match entry {
            std::collections::hash_map::Entry::Occupied(mut occ) => {
                // Already present — just union the role.
                let (_, roles) = occ.get_mut();
                if !roles.contains(&role) {
                    roles.push(role);
                }
            }
            std::collections::hash_map::Entry::Vacant(vac) => {
                let pr = raw_pr_to_domain(raw);
                vac.insert((pr, vec![role]));
            }
        }
    };

    // Authored PRs.
    for raw in data.authored.pull_requests.nodes {
        upsert(raw, Role::Author);
    }

    // Review-requested PRs.
    for node in data.review_requested.nodes.into_iter().flatten() {
        if let SearchNode::Pr(raw) = node {
            upsert(raw, Role::Reviewer);
        }
    }

    // Assigned PRs.
    for node in data.assigned_prs.nodes.into_iter().flatten() {
        if let SearchNode::Pr(raw) = node {
            upsert(raw, Role::Assignee);
        }
    }

    // Materialise PRs, attaching the final deduplicated roles.
    let mut prs: Vec<PullRequest> = pr_map
        .into_values()
        .map(|(mut pr, roles)| {
            pr.roles = roles;
            pr
        })
        .collect();
    // Sort by updated_at descending for a stable, predictable order.
    prs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    // Issues.
    let mut issues: Vec<Issue> = data
        .assigned_issues
        .nodes
        .into_iter()
        .flatten()
        .filter_map(|node| {
            if let SearchNode::Issue(raw) = node { Some(raw_issue_to_domain(raw)) } else { None }
        })
        .collect();
    issues.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Inbox { viewer_login, prs, issues }
}

// ── Private helpers ────────────────────────────────────────────────────────────

fn raw_pr_to_domain(raw: RawPr) -> PullRequest {
    let rollup = raw.commits.nodes.into_iter().next().and_then(|n| n.commit.status_check_rollup);

    let check_state = rollup.as_ref().map(|r| r.state);

    let failing_checks = rollup
        .map(|r| {
            r.contexts
                .nodes
                .into_iter()
                .filter_map(|ctx| match ctx {
                    RawCheckContext::CheckRun(cr) => {
                        let is_failing = cr
                            .conclusion
                            .as_deref()
                            .is_some_and(|c| matches!(c, "failure" | "error"));
                        if is_failing {
                            // Traverse: checkSuite → workflowRun → workflow → name
                            let workflow_name = cr
                                .check_suite
                                .as_ref()
                                .and_then(|cs| cs.workflow_run.as_ref())
                                .and_then(|wr| wr.workflow.as_ref())
                                .map(|w| w.name.clone());
                            Some(CheckRun {
                                name: cr.name,
                                workflow_name,
                                conclusion: cr.conclusion,
                                status: cr.status,
                            })
                        } else {
                            None
                        }
                    }
                    RawCheckContext::StatusContext(_) => None,
                })
                .collect()
        })
        .unwrap_or_default();

    // `count()` returns `usize`; review_threads is capped at 30 by the query
    // so truncation is impossible in practice.
    #[allow(clippy::cast_possible_truncation)]
    let unresolved_threads =
        raw.review_threads.nodes.iter().filter(|t| !t.is_resolved && !t.is_outdated).count() as u32;

    let requested_reviewers = raw
        .review_requests
        .nodes
        .into_iter()
        .filter_map(|rr| rr.requested_reviewer)
        .map(|rv| match rv {
            RawReviewer::User { login } => login,
            RawReviewer::Team { name } => name,
        })
        .collect();

    let reviews = raw
        .latest_reviews
        .nodes
        .into_iter()
        .filter_map(|r| r.author.map(|a| Review { author: a.login, state: r.state }))
        .collect();

    PullRequest {
        number: raw.number,
        title: raw.title,
        url: raw.url,
        repo: raw.repository.name_with_owner,
        author: raw.author.map(|a| a.login).unwrap_or_default(),
        is_draft: raw.is_draft,
        mergeable: raw.mergeable,
        merge_state: raw.merge_state_status,
        review_decision: raw.review_decision,
        commits_count: raw.commits.total_count,
        comments_count: raw.comments.total_count,
        check_state,
        failing_checks,
        unresolved_threads,
        requested_reviewers,
        reviews,
        updated_at: raw.updated_at,
        roles: vec![], // populated by the dedup step in to_inbox
    }
}

fn raw_issue_to_domain(raw: RawIssue) -> Issue {
    Issue {
        number: raw.number,
        title: raw.title,
        url: raw.url,
        repo: raw.repository.name_with_owner,
        author: raw.author.map(|a| a.login).unwrap_or_default(),
        comments_count: raw.comments.total_count,
        updated_at: raw.updated_at,
        labels: raw
            .labels
            .nodes
            .into_iter()
            .map(|l| Label { name: l.name, color: l.color })
            .collect(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn make_base_pr_json(
        number: u32,
        check_state: &str,
        conclusion: &str,
        review_decision: &str,
        is_draft: bool,
    ) -> serde_json::Value {
        serde_json::json!({
            "number": number,
            "title": "Test PR",
            "url": "https://github.com/owner/repo/pull/1",
            "isDraft": is_draft,
            "mergeable": "MERGEABLE",
            "mergeStateStatus": "CLEAN",
            "reviewDecision": review_decision,
            "repository": { "nameWithOwner": "owner/repo" },
            "author": { "login": "author-login" },
            "updatedAt": "2024-01-01T00:00:00Z",
            "commits": {
                "totalCount": 1,
                "nodes": [{
                    "commit": {
                        "statusCheckRollup": {
                            "state": check_state,
                            "contexts": {
                                "nodes": [{
                                    "name": "CI",
                                    "status": "COMPLETED",
                                    "conclusion": conclusion,
                                    "checkSuite": null
                                }]
                            }
                        }
                    }
                }]
            },
            "comments": { "totalCount": 0 },
            "reviewRequests": { "nodes": [] },
            "reviewThreads": { "nodes": [] },
            "latestReviews": { "nodes": [] }
        })
    }

    /// A PR with failing CI and `CHANGES_REQUESTED` review decision must
    /// deserialize correctly and produce the right domain values.
    #[test]
    fn failing_ci_and_changes_requested() {
        let json = make_base_pr_json(1, "FAILURE", "failure", "CHANGES_REQUESTED", false);
        let raw: RawPr = serde_json::from_value(json).expect("deserialize RawPr");
        let pr = raw_pr_to_domain(raw);

        assert_eq!(pr.check_state, Some(CheckState::Failure));
        assert_eq!(pr.review_decision, Some(ReviewDecision::ChangesRequested));
        assert_eq!(pr.failing_checks.len(), 1);
        assert_eq!(pr.failing_checks[0].name, "CI");
    }

    /// A clean, approved PR must have an empty `failing_checks` list.
    #[test]
    fn clean_approved_pr() {
        let json = make_base_pr_json(2, "SUCCESS", "success", "APPROVED", false);
        let raw: RawPr = serde_json::from_value(json).expect("deserialize RawPr");
        let pr = raw_pr_to_domain(raw);

        assert_eq!(pr.check_state, Some(CheckState::Success));
        assert_eq!(pr.review_decision, Some(ReviewDecision::Approved));
        assert!(pr.failing_checks.is_empty(), "clean PR should have no failing checks");
    }

    /// PRs with the same `(repo, number)` appearing in two buckets must be
    /// merged into one entry with both roles.
    #[test]
    fn dedup_unions_roles() {
        let pr_json = make_base_pr_json(1, "SUCCESS", "success", "APPROVED", false);
        let raw1: RawPr = serde_json::from_value(pr_json.clone()).expect("deserialize");
        let raw2: RawPr = serde_json::from_value(pr_json).expect("deserialize");

        let data = ResponseData {
            authored: AuthoredViewer {
                login: "viewer".to_owned(),
                pull_requests: NodeList { nodes: vec![raw1] },
            },
            review_requested: SearchResult { nodes: vec![Some(SearchNode::Pr(raw2))] },
            assigned_prs: SearchResult { nodes: vec![] },
            assigned_issues: SearchResult { nodes: vec![] },
        };

        let inbox = to_inbox("viewer".to_owned(), data);
        assert_eq!(inbox.prs.len(), 1, "duplicate PR must be merged");
        let roles = &inbox.prs[0].roles;
        assert!(roles.contains(&Role::Author), "Author role missing");
        assert!(roles.contains(&Role::Reviewer), "Reviewer role missing");
    }
}
