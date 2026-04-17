//! Async GitHub GraphQL HTTP client.

use std::sync::OnceLock;

use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Serialize;
use tracing::debug;

use super::detail::{
    ISSUE_DETAIL_QUERY, IssueDetail, PR_DETAIL_QUERY, PrDetail, RawDetailResponse,
    raw_issue_to_detail, raw_pr_to_detail,
};
use super::query::{GraphQlResponse, INBOX_QUERY};
use super::types::Inbox;

/// Version string embedded in the `User-Agent` header.
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The GitHub GraphQL endpoint.
const GRAPHQL_URL: &str = "https://api.github.com/graphql";

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

// ── Client ────────────────────────────────────────────────────────────────────

/// Authenticated GitHub GraphQL client.
///
/// Cheap to clone — the inner [`reqwest::Client`] uses an `Arc` internally
/// and the token is a plain `String`.
pub struct Client {
    http: reqwest::Client,
    token: String,
    /// Cached viewer login, populated lazily on the first successful fetch.
    viewer_login: OnceLock<String>,
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
        let response = self
            .http
            .post(GRAPHQL_URL)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, format!("octopeek/{PKG_VERSION}"))
            .header(ACCEPT, "application/vnd.github+json")
            .json(&NoVarBody { query: INBOX_QUERY })
            .send()
            .await
            .context("network error reaching GitHub GraphQL API")?;

        let status = response.status();

        // Handle HTTP-level errors before attempting to parse the body.
        check_http_status(status, response.headers())?;

        let gql: GraphQlResponse =
            response.json().await.context("failed to parse GitHub GraphQL response")?;

        // Log point cost if present (GitHub includes this in extensions).
        // The field path is `extensions.cost.actualCost` in the GitHub API.
        // We avoid pulling in a full `extensions` struct for a single debug log.
        debug!("GraphQL response received (status {status})");

        // Surface GraphQL application-level errors.
        if let Some(errors) = gql.errors
            && !errors.is_empty()
        {
            let messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
            anyhow::bail!("GitHub GraphQL errors: {}", messages.join("; "));
        }

        let data = gql.data.context("GitHub GraphQL response had no `data` field")?;

        // Cache the viewer login for callers that need it without a re-fetch.
        let viewer_login = data.authored.login.clone();
        let _ = self.viewer_login.set(viewer_login.clone());

        Ok(super::query::to_inbox(viewer_login, data))
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
        let gql = self.post_graphql_detail(PR_DETAIL_QUERY, owner, name, number).await?;

        let repository = gql
            .data
            .context("GitHub GraphQL response had no `data` field")?
            .repository
            .with_context(|| format!("repository `{repo}` not found or not accessible"))?;

        let raw_pr = repository
            .pull_request
            .with_context(|| format!("pull request #{number} not found in `{repo}`"))?;

        debug!("PR detail fetched: {repo}#{number}");
        Ok(raw_pr_to_detail(repo.to_owned(), raw_pr))
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
        let gql = self.post_graphql_detail(ISSUE_DETAIL_QUERY, owner, name, number).await?;

        let repository = gql
            .data
            .context("GitHub GraphQL response had no `data` field")?
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

    /// POST a parameterised detail query (`owner`, `name`, `number`) and return
    /// the parsed [`RawDetailResponse`], surfacing HTTP and GraphQL errors.
    ///
    /// Shared by [`Self::fetch_pr_detail`] and [`Self::fetch_issue_detail`] to
    /// avoid duplicating the HTTP + error-handling boilerplate.
    async fn post_graphql_detail(
        &self,
        query: &str,
        owner: &str,
        name: &str,
        number: u32,
    ) -> Result<RawDetailResponse> {
        let body = DetailBody { query, variables: DetailVariables { owner, name, number } };

        let response = self
            .http
            .post(GRAPHQL_URL)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, format!("octopeek/{PKG_VERSION}"))
            .header(ACCEPT, "application/vnd.github+json")
            .json(&body)
            .send()
            .await
            .context("network error reaching GitHub GraphQL API")?;

        let status = response.status();
        check_http_status(status, response.headers())?;

        let gql: RawDetailResponse =
            response.json().await.context("failed to parse GitHub GraphQL response")?;

        if let Some(errors) = gql.errors.as_ref()
            && !errors.is_empty()
        {
            let messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
            anyhow::bail!("GitHub GraphQL errors: {}", messages.join("; "));
        }

        Ok(gql)
    }
}

// ── Module-level helpers ──────────────────────────────────────────────────────

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
        anyhow::bail!("GitHub GraphQL API returned HTTP {status}");
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
}
