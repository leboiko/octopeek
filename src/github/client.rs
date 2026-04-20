//! Async GitHub GraphQL HTTP client.

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::detail::{
    FileChange, FileChangeKind, ISSUE_DETAIL_QUERY, IssueDetail, PR_DETAIL_QUERY, PrDetail,
    RawDetailData, raw_issue_to_detail, raw_pr_to_detail,
};
use super::mutations::{MergeMethod, MergeOutcome};
use super::query::{GqlEnvelope, ResponseData, ResponseDataAll, build_show_all_query, inbox_query};
use super::types::Inbox;

/// Version string embedded in the `User-Agent` header.
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The GitHub GraphQL endpoint.
const GRAPHQL_URL: &str = "https://api.github.com/graphql";

/// Base URL for the GitHub REST API v3.
const REST_BASE_URL: &str = "https://api.github.com";

/// GitHub REST API version used for mutating endpoints.
const REST_API_VERSION: &str = "2022-11-28";

/// Maximum page size accepted by GitHub REST list endpoints.
const REST_PAGE_SIZE: u32 = 100;

/// GitHub caps the pull-request files endpoint at 3,000 files.
const PR_FILES_REST_CAP: u32 = 3_000;

// ── Shared serialization types ────────────────────────────────────────────────

/// Request body for GraphQL queries that take no variables.
#[derive(Serialize)]
struct NoVarBody<'a> {
    query: &'a str,
}

/// Request body for GraphQL queries parameterized by `owner`, `name`, and `number`.
#[derive(Serialize)]
struct DetailBody<'a> {
    query: &'a str,
    variables: DetailVariables<'a>,
}

/// Variables sent alongside a detail query.
#[derive(Serialize)]
struct DetailVariables<'a> {
    owner: &'a str,
    name: &'a str,
    number: u32,
}

/// Request body for the top-level add-comment mutation.
#[derive(Serialize)]
struct AddCommentBody<'a> {
    query: &'a str,
    variables: AddCommentVariables<'a>,
}

/// Variables for the top-level add-comment mutation.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AddCommentVariables<'a> {
    subject_id: &'a str,
    body: &'a str,
}

/// Request body for the review-thread reply mutation.
#[derive(Serialize)]
struct ThreadReplyBody<'a> {
    query: &'a str,
    variables: ThreadReplyVariables<'a>,
}

/// Variables for the review-thread reply mutation.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadReplyVariables<'a> {
    thread_id: &'a str,
    body: &'a str,
}

/// REST body for `PUT /repos/{owner}/{repo}/pulls/{number}/merge`.
#[derive(Serialize)]
struct RestMergeBody<'a> {
    sha: &'a str,
    merge_method: &'a str,
}

/// One entry from the REST `GET /repos/{owner}/{repo}/pulls/{number}/files` response.
///
/// GitHub omits `patch` (or sets it to `null`) for binary files and diffs that
/// exceed the inline size limit. `#[serde(default)]` maps both absent and `null`
/// to `None`.
#[derive(Deserialize)]
struct RestFileEntry {
    filename: String,
    #[serde(default)]
    status: String,
    additions: u32,
    deletions: u32,
    #[serde(default)]
    patch: Option<String>,
}

/// Wrapper for the REST `GET /repos/{owner}/{name}/commits/{sha}` response body.
///
/// Only the `files` array is consumed; all other commit metadata fields are
/// ignored (GitHub's GraphQL already gave us those).
#[derive(Deserialize)]
struct CommitResponse {
    #[serde(default)]
    files: Vec<RestFileEntry>,
}

/// Minimal response from GitHub's REST PR merge endpoint.
#[derive(Deserialize)]
struct RestMergeResponse {
    sha: String,
    message: String,
}

/// Common JSON error shape returned by GitHub REST endpoints.
#[derive(Deserialize)]
struct RestErrorResponse {
    message: Option<String>,
}

/// Minimal GraphQL payload used by comment mutations.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddCommentData {
    add_comment: MinimalMutationPayload,
}

/// Minimal GraphQL payload used by review-thread reply mutations.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddThreadReplyData {
    add_pull_request_review_thread_reply: MinimalMutationPayload,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MinimalMutationPayload {
    #[allow(dead_code)]
    client_mutation_id: Option<String>,
}

const ADD_COMMENT_MUTATION: &str = r"
mutation AddComment($subjectId: ID!, $body: String!) {
  addComment(input: {subjectId: $subjectId, body: $body}) {
    clientMutationId
  }
}
";

const ADD_THREAD_REPLY_MUTATION: &str = r"
mutation AddPullRequestReviewThreadReply($threadId: ID!, $body: String!) {
  addPullRequestReviewThreadReply(input: {pullRequestReviewThreadId: $threadId, body: $body}) {
    clientMutationId
  }
}
";

// ── Client ────────────────────────────────────────────────────────────────────

/// Authenticated GitHub GraphQL client.
///
/// Cheap to clone — the inner [`reqwest::Client`] uses an `Arc` internally
/// and the token is a plain `String`.
///
/// The [`Debug`] impl redacts the `token` field, so a stray `{:?}` format
/// (or a future `tracing::debug!(?client, …)`) cannot leak the bearer token
/// into logs. Do not add `#[derive(Debug)]` — it would undo this guard.
pub struct Client {
    http: reqwest::Client,
    token: String,
    /// Cached viewer login, populated lazily on the first successful fetch.
    viewer_login: OnceLock<String>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("http", &"reqwest::Client")
            .field("token", &"[REDACTED]")
            .field("viewer_login", &self.viewer_login)
            .finish()
    }
}

impl Client {
    /// Construct a new client with the given token.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying `reqwest::Client` cannot be built
    /// (extremely rare — only if TLS initialisation fails).
    pub fn new(token: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            // A 30-second cap keeps a hung GraphQL endpoint from pinning the
            // fetch task indefinitely, which would strand `App::fetching` on
            // `true` and block every subsequent refresh.
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self { http, token, viewer_login: OnceLock::new() })
    }

    /// Fetch the viewer's inbox from GitHub's GraphQL API.
    ///
    /// # Errors
    ///
    /// - 401 → token invalid or expired.
    /// - 403 with rate-limit headers → rate limit reached.
    /// - Network errors → wrapped with context.
    /// - GraphQL `errors` array → joined and returned as an error.
    pub async fn fetch_inbox(&self) -> Result<Inbox> {
        let data: ResponseData =
            self.post_graphql(&NoVarBody { query: inbox_query() }, "inbox").await?;

        // Cache the viewer login for callers that need it without a re-fetch.
        let viewer_login = data.authored.login.clone();
        let _ = self.viewer_login.set(viewer_login.clone());

        Ok(super::query::to_inbox(viewer_login, data))
    }

    /// Fetch every open PR and issue across the given list of repositories.
    ///
    /// Unlike [`Self::fetch_inbox`], this query is not scoped to `@me`; it
    /// returns all open items for each tracked repo so the user gets a
    /// full-team view. Roles on the returned [`crate::github::types::PullRequest`] values are derived
    /// from author / review-request fields — see [`super::query::to_inbox_all`]
    /// for the exact derivation logic.
    ///
    /// # Arguments
    ///
    /// * `repos` - Slice of repo slugs in `owner/name` form. An empty slice
    ///   produces a valid (but empty) result.
    ///
    /// # Errors
    ///
    /// Same error conditions as [`Self::fetch_inbox`].
    pub async fn fetch_inbox_all(&self, repos: &[String]) -> Result<Inbox> {
        let query = build_show_all_query(repos);
        let data: ResponseDataAll =
            self.post_graphql(&NoVarBody { query: &query }, "show-all").await?;

        let viewer_login = data.viewer.login.clone();
        let _ = self.viewer_login.set(viewer_login.clone());

        Ok(super::query::to_inbox_all(viewer_login, data))
    }

    /// Fetch the full detail for a single pull request.
    ///
    /// # Arguments
    ///
    /// * `repo`   - Repository slug in `owner/name` form.
    /// * `number` - PR number within the repository.
    ///
    /// # Errors
    ///
    /// - `repo` does not contain exactly one `/` → descriptive error.
    /// - 401 / 403 / rate-limit → same handling as [`Self::fetch_inbox`].
    /// - `repository` or `pullRequest` is `null` → descriptive error.
    pub async fn fetch_pr_detail(&self, repo: &str, number: u32) -> Result<PrDetail> {
        let (owner, name) = split_repo(repo)?;
        let data = self.post_graphql_detail(PR_DETAIL_QUERY, owner, name, number).await?;

        let repository = data
            .repository
            .with_context(|| format!("repository `{repo}` not found or not accessible"))?;

        let raw_pr = repository
            .pull_request
            .with_context(|| format!("pull request #{number} not found in `{repo}`"))?;

        debug!("PR detail fetched: {repo}#{number}");
        let mut detail = raw_pr_to_detail(repo.to_owned(), raw_pr);

        // Replace the GraphQL `files(first: 100)` fallback with the paginated
        // REST file list. GitHub's GraphQL connection can only return 100
        // nodes per request; the REST endpoint pages through the full changed
        // file list (up to GitHub's documented 3,000-file cap) and carries the
        // same inline patch text used by the diff renderer.
        match self.fetch_pr_files(owner, name, number, detail.changed_files_count).await {
            Ok(files) => detail.files = files,
            Err(err) => {
                warn!(
                    repo,
                    number,
                    error = %err,
                    "REST file-list fetch failed; GraphQL files fallback may be truncated"
                );
            }
        }

        Ok(detail)
    }

    /// Fetch the full detail for a single issue.
    ///
    /// # Arguments
    ///
    /// * `repo`   - Repository slug in `owner/name` form.
    /// * `number` - Issue number within the repository.
    ///
    /// # Errors
    ///
    /// - `repo` does not contain exactly one `/` → descriptive error.
    /// - 401 / 403 / rate-limit → same handling as [`Self::fetch_inbox`].
    /// - `repository` or `issue` is `null` → descriptive error.
    pub async fn fetch_issue_detail(&self, repo: &str, number: u32) -> Result<IssueDetail> {
        let (owner, name) = split_repo(repo)?;
        let data = self.post_graphql_detail(ISSUE_DETAIL_QUERY, owner, name, number).await?;

        let repository = data
            .repository
            .with_context(|| format!("repository `{repo}` not found or not accessible"))?;

        let raw_issue =
            repository.issue.with_context(|| format!("issue #{number} not found in `{repo}`"))?;

        debug!("Issue detail fetched: {repo}#{number}");
        Ok(raw_issue_to_detail(repo.to_owned(), raw_issue))
    }

    /// Return the viewer login if it has been populated by a prior fetch.
    // Used by the Phase 3 UI to personalise the dashboard header.
    #[allow(dead_code)]
    pub fn cached_viewer_login(&self) -> Option<&str> {
        self.viewer_login.get().map(String::as_str)
    }

    /// Fetch the file-level diff introduced by a single commit.
    ///
    /// Calls `GET /repos/{owner}/{name}/commits/{sha}` and extracts the
    /// `files[].patch` entries, mapping filename → `Option<patch>`.
    /// `None` for the inner `Option` indicates a binary file or a diff that
    /// GitHub refuses to inline (same semantics as the PR-files endpoint).
    ///
    /// # Arguments
    ///
    /// * `repo` - Repository slug in `owner/name` form.
    /// * `sha`  - Full 40-character commit SHA.
    ///
    /// # Errors
    ///
    /// - `repo` cannot be split into `owner/name` → descriptive error.
    /// - Network failure, non-2xx HTTP, or JSON parse failure.
    pub(crate) async fn fetch_commit_diff(
        &self,
        repo: &str,
        sha: &str,
    ) -> Result<HashMap<String, Option<String>>> {
        let (owner, name) = split_repo(repo)?;
        let url = format!("{REST_BASE_URL}/repos/{owner}/{name}/commits/{sha}");

        let response = self
            .http
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, format!("octopeek/{PKG_VERSION}"))
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .context("network error reaching GitHub REST API (commit diff)")?;

        let status = response.status();
        check_http_status(status, response.headers())?;

        // The commits endpoint returns `{ "files": [...] }` rather than a
        // bare array like the pulls/files endpoint does.
        let body: CommitResponse =
            response.json().await.context("failed to parse GitHub REST commit response")?;

        let map = body.files.into_iter().map(|e| (e.filename, e.patch)).collect();
        Ok(map)
    }

    /// Merge a pull request using GitHub's REST API.
    ///
    /// `expected_head_sha` is sent as the endpoint's `sha` guard so the merge
    /// fails instead of racing through a force-push or late commit.
    pub(crate) async fn merge_pull_request(
        &self,
        repo: &str,
        number: u32,
        method: MergeMethod,
        expected_head_sha: &str,
    ) -> Result<MergeOutcome> {
        let (owner, name) = split_repo(repo)?;
        let url = format!("{REST_BASE_URL}/repos/{owner}/{name}/pulls/{number}/merge");
        let body = RestMergeBody { sha: expected_head_sha, merge_method: method.rest_value() };

        let response = self
            .http
            .put(&url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, format!("octopeek/{PKG_VERSION}"))
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", REST_API_VERSION)
            .json(&body)
            .send()
            .await
            .context("network error reaching GitHub REST API (merge pull request)")?;

        let status = response.status();
        if !status.is_success() {
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                check_http_status(status, response.headers())?;
            }
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read error response body".to_owned());
            let message = serde_json::from_str::<RestErrorResponse>(&text)
                .ok()
                .and_then(|body| body.message)
                .unwrap_or(text);
            anyhow::bail!("GitHub merge API returned HTTP {status}: {message}");
        }

        let body: RestMergeResponse =
            response.json().await.context("failed to parse GitHub REST merge response")?;

        Ok(MergeOutcome { sha: body.sha, message: body.message })
    }

    /// Add a top-level comment to a pull request or issue by GraphQL node ID.
    pub(crate) async fn add_comment(&self, subject_id: &str, body: &str) -> Result<()> {
        let variables = AddCommentVariables { subject_id, body };
        let data: AddCommentData = self
            .post_graphql(&AddCommentBody { query: ADD_COMMENT_MUTATION, variables }, "add-comment")
            .await?;
        let _ = data.add_comment;
        Ok(())
    }

    /// Reply to an existing pull request review thread by GraphQL node ID.
    pub(crate) async fn reply_to_review_thread(&self, thread_id: &str, body: &str) -> Result<()> {
        let variables = ThreadReplyVariables { thread_id, body };
        let data: AddThreadReplyData = self
            .post_graphql(
                &ThreadReplyBody { query: ADD_THREAD_REPLY_MUTATION, variables },
                "reply-review-thread",
            )
            .await?;
        let _ = data.add_pull_request_review_thread_reply;
        Ok(())
    }

    /// Fetch the full PR file list from the GitHub REST API.
    ///
    /// Calls `GET /repos/{owner}/{name}/pulls/{number}/files?per_page=100`
    /// and follows numeric pages until all changed files have been loaded or
    /// GitHub's 3,000-file endpoint cap is reached.
    ///
    /// # Returns
    ///
    /// A vector of [`FileChange`] values carrying path, change kind, counts,
    /// and optional patch text.
    ///
    /// # Errors
    ///
    /// Network errors, non-2xx HTTP responses, or JSON parse failures.
    async fn fetch_pr_files(
        &self,
        owner: &str,
        name: &str,
        number: u32,
        expected_total: u32,
    ) -> Result<Vec<FileChange>> {
        if expected_total == 0 {
            return Ok(Vec::new());
        }

        let capped_total = expected_total.min(PR_FILES_REST_CAP);
        let total_pages = capped_total.div_ceil(REST_PAGE_SIZE).max(1);
        let mut files = Vec::with_capacity(capped_total as usize);

        for page in 1..=total_pages {
            let url = format!(
                "{REST_BASE_URL}/repos/{owner}/{name}/pulls/{number}/files?per_page={REST_PAGE_SIZE}&page={page}"
            );

            let response = self
                .http
                .get(&url)
                .header(AUTHORIZATION, format!("Bearer {}", self.token))
                .header(USER_AGENT, format!("octopeek/{PKG_VERSION}"))
                .header(ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", REST_API_VERSION)
                .send()
                .await
                .context("network error reaching GitHub REST API (pull request files)")?;

            let status = response.status();
            check_http_status(status, response.headers())?;

            let entries: Vec<RestFileEntry> =
                response.json().await.context("failed to parse GitHub REST files response")?;
            let page_len = entries.len();
            files.extend(entries.into_iter().map(rest_file_entry_to_change));

            if page_len < REST_PAGE_SIZE as usize || files.len() >= capped_total as usize {
                break;
            }
        }

        Ok(files)
    }

    /// POST a parameterised detail query (`owner`, `name`, `number`) and return
    /// the inner `data` struct, surfacing HTTP and GraphQL errors.
    ///
    /// Shared by [`Self::fetch_pr_detail`] and [`Self::fetch_issue_detail`].
    async fn post_graphql_detail(
        &self,
        query: &str,
        owner: &str,
        name: &str,
        number: u32,
    ) -> Result<RawDetailData> {
        let body = DetailBody { query, variables: DetailVariables { owner, name, number } };
        self.post_graphql(&body, "detail").await
    }

    /// POST any GraphQL query and return the deserialized `data` field.
    ///
    /// Centralises the four steps every GraphQL call repeats:
    /// 1. Attach auth / user-agent / accept headers.
    /// 2. Check the HTTP status and rate-limit headers.
    /// 3. Parse the response as [`GqlEnvelope<T>`].
    /// 4. Surface any non-empty `errors[]` array as an `anyhow` error.
    ///
    /// The caller owns the body type (e.g. [`NoVarBody`], [`DetailBody`]) and
    /// the inner response shape `T`. `label` is only used to tag the success
    /// trace log (`"GraphQL {label} response received"`) and has no effect on
    /// behaviour.
    ///
    /// # Errors
    ///
    /// - Network failure, non-success HTTP, invalid JSON body.
    /// - Any `errors[]` entry present in the response.
    /// - Response with `data == null` (no recoverable information).
    async fn post_graphql<B: Serialize + ?Sized, T: serde::de::DeserializeOwned>(
        &self,
        body: &B,
        label: &str,
    ) -> Result<T> {
        let response = self
            .http
            .post(GRAPHQL_URL)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, format!("octopeek/{PKG_VERSION}"))
            .header(ACCEPT, "application/vnd.github+json")
            .json(body)
            .send()
            .await
            .context("network error reaching GitHub GraphQL API")?;

        let status = response.status();
        check_http_status(status, response.headers())?;

        let gql: GqlEnvelope<T> =
            response.json().await.context("failed to parse GitHub GraphQL response")?;

        debug!("GraphQL {label} response received (status {status})");

        if let Some(errors) = gql.errors
            && !errors.is_empty()
        {
            let messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
            anyhow::bail!("GitHub GraphQL errors: {}", messages.join("; "));
        }

        gql.data.context("GitHub GraphQL response had no `data` field")
    }
}

// ── Module-level helpers ──────────────────────────────────────────────────────

fn rest_file_entry_to_change(entry: RestFileEntry) -> FileChange {
    FileChange {
        path: entry.filename,
        additions: entry.additions,
        deletions: entry.deletions,
        change_kind: parse_rest_file_status(&entry.status),
        patch: entry.patch,
    }
}

fn parse_rest_file_status(status: &str) -> FileChangeKind {
    match status {
        "added" => FileChangeKind::Added,
        "removed" => FileChangeKind::Deleted,
        "renamed" => FileChangeKind::Renamed,
        "copied" => FileChangeKind::Copied,
        "modified" => FileChangeKind::Modified,
        // `changed` is rare, and unknown future values should still render.
        _ => FileChangeKind::Changed,
    }
}

/// Split `"owner/name"` into `("owner", "name")`.
///
/// Returns an error if the slug has no `/` or has more than one `/`.
fn split_repo(repo: &str) -> Result<(&str, &str)> {
    let mut parts = repo.splitn(2, '/');
    let owner = parts
        .next()
        .filter(|s| !s.is_empty())
        .context("invalid repo slug: expected `owner/name`")?;
    let name = parts
        .next()
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .with_context(|| format!("invalid repo slug `{repo}`: expected exactly one `/`"))?;
    Ok((owner, name))
}

/// Translate HTTP-level error codes into descriptive `anyhow` errors.
///
/// Called after every GraphQL POST before attempting to decode the body.
fn check_http_status(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
) -> Result<()> {
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("GitHub returned 401: token invalid or expired. Run `gh auth login`.");
    }

    if status == reqwest::StatusCode::FORBIDDEN {
        let reset = headers
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<i64>().ok())
            .map_or_else(
                || "unknown".to_owned(),
                |ts| {
                    chrono::DateTime::from_timestamp(ts, 0)
                        .map_or_else(|| ts.to_string(), |dt| dt.to_rfc3339())
                },
            );
        anyhow::bail!("GitHub API rate limit reached. Resets at {reset}.");
    }

    if !status.is_success() {
        anyhow::bail!("GitHub API returned HTTP {status}");
    }

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// A repo slug with no `/` must return a clear error.
    #[test]
    fn split_repo_no_slash_errors() {
        let err = split_repo("nodash").expect_err("should fail");
        assert!(err.to_string().contains("expected exactly one"), "error: {err}");
    }

    /// A valid slug splits cleanly.
    #[test]
    fn split_repo_valid() {
        let (owner, name) = split_repo("leboiko/octopeek").expect("should succeed");
        assert_eq!(owner, "leboiko");
        assert_eq!(name, "octopeek");
    }

    /// An empty owner segment must return an error.
    #[test]
    fn split_repo_empty_owner_errors() {
        let err = split_repo("/name").expect_err("should fail");
        assert!(err.to_string().contains("owner/name"), "error: {err}");
    }

    /// An empty name segment must return an error.
    #[test]
    fn split_repo_empty_name_errors() {
        let err = split_repo("owner/").expect_err("should fail");
        assert!(err.to_string().contains("expected exactly one"), "error: {err}");
    }

    /// The manual `Debug` impl redacts the bearer token.
    ///
    /// Guards against a regression where `#[derive(Debug)]` silently exposes
    /// the token in any `tracing::debug!(?client, …)` or `format!("{:?}")`
    /// call. If this test breaks, do not "fix" it by tweaking the assertion
    /// — restore the manual `Debug` impl.
    #[test]
    fn debug_impl_redacts_token() {
        let secret = "ghp_supersecret_must_not_leak";
        let client = Client::new(secret.to_owned()).expect("client build");
        let rendered = format!("{client:?}");
        assert!(
            !rendered.contains(secret),
            "Debug output must not contain the token; got: {rendered}"
        );
        assert!(
            rendered.contains("[REDACTED]"),
            "Debug output must show a [REDACTED] placeholder; got: {rendered}"
        );
    }

    // ── REST PR-files data layer ──────────────────────────────────────────────

    /// Deserialise a representative REST files response containing one normal
    /// file (with a patch) and one binary file (patch absent / null).
    #[test]
    fn rest_files_response_deserializes_basic() {
        let json = r#"[
            {
                "filename": "src/main.rs",
                "status": "modified",
                "additions": 5,
                "deletions": 2,
                "patch": "@@ -1,3 +1,6 @@\n+new line"
            },
            {
                "filename": "assets/logo.png",
                "status": "added",
                "additions": 0,
                "deletions": 0
            }
        ]"#;

        let entries: Vec<RestFileEntry> = serde_json::from_str(json).expect("deserialise");

        let files: Vec<FileChange> = entries.into_iter().map(rest_file_entry_to_change).collect();

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].change_kind, FileChangeKind::Modified);
        assert!(files[0].patch.is_some(), "text file should have a patch");
        assert_eq!(files[1].path, "assets/logo.png");
        assert_eq!(files[1].change_kind, FileChangeKind::Added);
        assert!(files[1].patch.is_none(), "binary file should have patch == None");
    }

    /// Explicit `patch: null` in the JSON payload must also produce `None`.
    #[test]
    fn rest_files_response_patch_null_becomes_none() {
        let json = r#"[{"filename": "big.rs", "additions": 0, "deletions": 0, "patch": null}]"#;
        let entries: Vec<RestFileEntry> = serde_json::from_str(json).expect("deserialise");
        let map: HashMap<String, Option<String>> =
            entries.into_iter().map(|e| (e.filename, e.patch)).collect();
        assert!(map["big.rs"].is_none());
    }

    /// REST status strings must map to the same file-change enum used by the UI.
    #[test]
    fn rest_file_status_maps_to_change_kind() {
        assert_eq!(parse_rest_file_status("added"), FileChangeKind::Added);
        assert_eq!(parse_rest_file_status("modified"), FileChangeKind::Modified);
        assert_eq!(parse_rest_file_status("removed"), FileChangeKind::Deleted);
        assert_eq!(parse_rest_file_status("renamed"), FileChangeKind::Renamed);
        assert_eq!(parse_rest_file_status("copied"), FileChangeKind::Copied);
        assert_eq!(parse_rest_file_status("unexpected"), FileChangeKind::Changed);
    }

    /// The `GET /repos/{owner}/{name}/commits/{sha}` REST response wraps the
    /// file list inside a `{ "files": [...] }` object, unlike the PR-files
    /// endpoint which returns a bare array. Verify that `CommitResponse`
    /// deserialises correctly for one text file (patch present) and one
    /// binary file (patch absent).
    #[test]
    fn commit_diff_fetch_parses_rest_response() {
        let json = r#"{
            "sha": "abc1234def5678abc1234def5678abc1234def56",
            "commit": { "message": "fix: something" },
            "files": [
                {
                    "filename": "src/main.rs",
                    "additions": 3,
                    "deletions": 1,
                    "patch": "@@ -1,3 +1,6 @@\n+new line"
                },
                {
                    "filename": "assets/image.png",
                    "additions": 0,
                    "deletions": 0
                }
            ]
        }"#;

        let body: CommitResponse = serde_json::from_str(json).expect("deserialise CommitResponse");
        assert_eq!(body.files.len(), 2);

        // Build the map the same way `fetch_commit_diff` does.
        let map: HashMap<String, Option<String>> =
            body.files.into_iter().map(|e| (e.filename, e.patch)).collect();

        assert!(map["src/main.rs"].is_some(), "text file must have a patch");
        assert!(map["assets/image.png"].is_none(), "binary file must have patch == None");
    }

    /// A `FileChange` constructed via the GraphQL path must default `patch` to `None`.
    #[test]
    fn file_change_patch_defaults_to_none() {
        use super::super::detail::{FileChange, FileChangeKind};

        let fc = FileChange {
            path: "src/foo.rs".to_owned(),
            additions: 1,
            deletions: 0,
            change_kind: FileChangeKind::Added,
            patch: None,
        };
        assert!(fc.patch.is_none());
    }
}
