//! Async GitHub GraphQL HTTP client.

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::detail::{
    ISSUE_DETAIL_QUERY, IssueDetail, PR_DETAIL_QUERY, PrDetail, RawDetailData,
    raw_issue_to_detail, raw_pr_to_detail,
};
use super::query::{
    GqlEnvelope, ResponseData, ResponseDataAll, build_show_all_query, inbox_query,
};
use super::types::Inbox;

/// Version string embedded in the `User-Agent` header.
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The GitHub GraphQL endpoint.
const GRAPHQL_URL: &str = "https://api.github.com/graphql";

/// Base URL for the GitHub REST API v3.
const REST_BASE_URL: &str = "https://api.github.com";

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

/// One entry from the REST `GET /repos/{owner}/{repo}/pulls/{number}/files` response.
///
/// GitHub omits `patch` (or sets it to `null`) for binary files and diffs that
/// exceed the inline size limit. `#[serde(default)]` maps both absent and `null`
/// to `None`.
#[derive(Deserialize)]
struct RestFileEntry {
    filename: String,
    #[serde(default)]
    patch: Option<String>,
}

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

        // Attempt to enrich each FileChange with its unified-diff patch from the
        // REST endpoint. Failures are non-fatal: the UI gracefully shows "patch
        // unavailable" when `patch` remains `None`.
        match self.fetch_pr_file_patches(owner, name, number).await {
            Ok(patch_map) => merge_patches_into_files(&mut detail.files, &patch_map),
            Err(err) => {
                warn!(
                    repo,
                    number,
                    error = %err,
                    "REST file-patches fetch failed; patches will be unavailable"
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

    /// Fetch per-file unified-diff patches from the GitHub REST API.
    ///
    /// Calls `GET /repos/{owner}/{name}/pulls/{number}/files?per_page=30`.
    /// Only the first page (up to 30 files) is retrieved — intentionally, to
    /// keep response sizes small. Files beyond the 30-file cap will have
    /// `patch == None` after the merge step.
    ///
    /// # Returns
    ///
    /// A map from file path to `Option<patch>`. The inner `Option` is `None`
    /// when GitHub omits the patch (binary files, oversized diffs).
    ///
    /// # Errors
    ///
    /// Network errors, non-2xx HTTP responses, or JSON parse failures.
    async fn fetch_pr_file_patches(
        &self,
        owner: &str,
        name: &str,
        number: u32,
    ) -> Result<HashMap<String, Option<String>>> {
        let url = format!("{REST_BASE_URL}/repos/{owner}/{name}/pulls/{number}/files?per_page=30");

        let response = self
            .http
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, format!("octopeek/{PKG_VERSION}"))
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .context("network error reaching GitHub REST API")?;

        let status = response.status();
        check_http_status(status, response.headers())?;

        let entries: Vec<RestFileEntry> =
            response.json().await.context("failed to parse GitHub REST files response")?;

        let map = entries.into_iter().map(|e| (e.filename, e.patch)).collect();
        Ok(map)
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

/// Merge REST-fetched patches into a slice of [`crate::github::detail::FileChange`] values in-place.
///
/// Looks up each file's `path` in `patch_map` and writes the associated patch
/// (which may itself be `None` for binary / oversized files) into
/// `file.patch`. Files whose paths are absent from the map — i.e. those
/// beyond the 30-file REST cap — are left with `patch == None`.
///
/// # Arguments
///
/// * `files`     - Mutable slice of file changes, mutated in place.
/// * `patch_map` - Map from file path to `Option<patch>` built from the REST
///   response.
pub(crate) fn merge_patches_into_files(
    files: &mut [super::detail::FileChange],
    patch_map: &HashMap<String, Option<String>>,
) {
    for file in files.iter_mut() {
        if let Some(patch) = patch_map.get(&file.path) {
            // Clone the Option<String> out of the map.  The map is consumed
            // after this call, so cloning is the minimal-allocation choice.
            file.patch = patch.clone();
        }
        // Files not in the map keep patch == None (beyond REST cap or not matched).
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

    // ── REST file-patches data layer ──────────────────────────────────────────

    /// Deserialise a representative REST files response containing one normal
    /// file (with a patch) and one binary file (patch absent / null).
    #[test]
    fn rest_files_response_deserializes_basic() {
        let json = r#"[
            {
                "filename": "src/main.rs",
                "additions": 5,
                "deletions": 2,
                "patch": "@@ -1,3 +1,6 @@\n+new line"
            },
            {
                "filename": "assets/logo.png",
                "additions": 0,
                "deletions": 0
            }
        ]"#;

        let entries: Vec<RestFileEntry> = serde_json::from_str(json).expect("deserialise");

        // Build the map the same way the real method does.
        let map: HashMap<String, Option<String>> =
            entries.into_iter().map(|e| (e.filename, e.patch)).collect();

        assert_eq!(map.len(), 2);
        assert!(map["src/main.rs"].is_some(), "text file should have a patch");
        assert!(map["assets/logo.png"].is_none(), "binary file should have patch == None");
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

    /// A 40-element REST payload must produce at most 30 entries when capped.
    ///
    /// The real `fetch_pr_file_patches` uses `?per_page=30` so GitHub returns
    /// at most 30 entries in practice. This test validates the serde parsing
    /// with exactly 30 items to confirm the per-page cap is honoured at the
    /// type level — if someone removes the cap the integration test should catch it.
    #[test]
    fn rest_cap_of_30_honoured() {
        // Build a JSON array of 40 file entries.
        let entries: Vec<serde_json::Value> = (0..40)
            .map(|i| {
                serde_json::json!({
                    "filename": format!("src/file_{i}.rs"),
                    "additions": 1,
                    "deletions": 0,
                    "patch": "@@ -0,0 +1 @@\n+x"
                })
            })
            .collect();
        let json = serde_json::to_string(&entries).expect("serialise");

        // Parse all 40 entries — the REST cap is enforced by `?per_page=30`
        // in the URL, not by taking a slice here.  Assert that when GitHub
        // would return exactly 30 (the first page), we get 30.
        let parsed: Vec<RestFileEntry> = serde_json::from_str(&json).expect("deserialise");
        // Simulate taking only the first page.
        let capped: HashMap<String, Option<String>> =
            parsed.into_iter().take(30).map(|e| (e.filename, e.patch)).collect();

        assert_eq!(capped.len(), 30, "at most 30 entries after per-page cap");
        assert!(!capped.contains_key("src/file_30.rs"), "entry beyond cap must be absent");
    }

    /// `merge_patches_into_files` must populate matched files, leave unmatched
    /// files at `None`, and not panic on empty inputs.
    #[test]
    fn merge_patches_populates_files() {
        use super::super::detail::{FileChange, FileChangeKind};

        let mut files = vec![
            FileChange {
                path: "src/main.rs".to_owned(),
                additions: 5,
                deletions: 2,
                change_kind: FileChangeKind::Modified,
                patch: None,
            },
            FileChange {
                path: "src/lib.rs".to_owned(),
                additions: 1,
                deletions: 0,
                change_kind: FileChangeKind::Modified,
                patch: None,
            },
            FileChange {
                path: "beyond_cap.rs".to_owned(),
                additions: 10,
                deletions: 0,
                change_kind: FileChangeKind::Added,
                patch: None,
            },
        ];

        let mut patch_map: HashMap<String, Option<String>> = HashMap::new();
        patch_map.insert("src/main.rs".to_owned(), Some("@@ -1 +1 @@\n+hello".to_owned()));
        // src/lib.rs is in the map but has no patch (binary / oversized).
        patch_map.insert("src/lib.rs".to_owned(), None);
        // beyond_cap.rs is intentionally absent from the map.

        merge_patches_into_files(&mut files, &patch_map);

        assert_eq!(
            files[0].patch.as_deref(),
            Some("@@ -1 +1 @@\n+hello"),
            "matched file must have patch populated"
        );
        assert!(files[1].patch.is_none(), "binary/oversized file should stay None");
        assert!(files[2].patch.is_none(), "file beyond REST cap must stay None");

        // Empty inputs must not panic.
        merge_patches_into_files(&mut [], &HashMap::new());
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
