//! Async GitHub GraphQL HTTP client.

use std::sync::OnceLock;

use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Serialize;
use tracing::debug;

use super::query::{GraphQlResponse, INBOX_QUERY};
use super::types::Inbox;

/// Version string embedded in the `User-Agent` header.
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The GitHub GraphQL endpoint.
const GRAPHQL_URL: &str = "https://api.github.com/graphql";

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
        #[derive(Serialize)]
        struct Body<'a> {
            query: &'a str,
        }

        let response = self
            .http
            .post(GRAPHQL_URL)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, format!("octopeek/{PKG_VERSION}"))
            .header(ACCEPT, "application/vnd.github+json")
            .json(&Body { query: INBOX_QUERY })
            .send()
            .await
            .context("network error reaching GitHub GraphQL API")?;

        let status = response.status();

        // Handle HTTP-level errors before attempting to parse the body.
        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("GitHub returned 401: token invalid or expired. Run `gh auth login`.");
        }

        if status == reqwest::StatusCode::FORBIDDEN {
            // Check for rate-limit headers.
            let reset = response
                .headers()
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

    /// Return the viewer login if it has been populated by a prior fetch.
    // Used by the Phase 3 UI to personalise the dashboard header.
    #[allow(dead_code)]
    pub fn cached_viewer_login(&self) -> Option<&str> {
        self.viewer_login.get().map(String::as_str)
    }
}
