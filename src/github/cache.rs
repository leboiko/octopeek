//! In-process detail cache with stale-while-revalidate semantics.
//!
//! Only PR and issue detail payloads are cached here. Inbox data is held
//! directly in `App::inbox`. Eviction is deferred to a later version — in
//! v0.1 entries live until the process exits.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::github::detail::{IssueDetail, PrDetail};

/// How long a cache entry is considered fresh before a background re-fetch is
/// triggered (stale-while-revalidate). The entry is still served immediately;
/// only the background kick changes.
pub const CACHE_TTL: Duration = Duration::from_secs(60);

// ── Cached<T> ─────────────────────────────────────────────────────────────────

/// A cached value together with the instant it was stored.
#[derive(Debug, Clone)]
pub struct Cached<T> {
    /// The stored payload.
    pub data: T,
    /// When this entry was inserted. Public so tests can manufacture stale
    /// entries by constructing `Cached { data, fetched_at: Instant::now() - large_duration }`.
    pub fetched_at: Instant,
}

impl<T> Cached<T> {
    /// Create a new entry stamped with the current time.
    pub fn new(data: T) -> Self {
        Self { data, fetched_at: Instant::now() }
    }

    /// How long ago this entry was stored.
    pub fn age(&self) -> Duration {
        self.fetched_at.elapsed()
    }

    /// `true` when the entry is younger than [`CACHE_TTL`] and no background
    /// revalidation is needed.
    pub fn is_fresh(&self) -> bool {
        self.age() < CACHE_TTL
    }
}

// ── DetailCache ───────────────────────────────────────────────────────────────

/// In-memory cache for PR and issue detail payloads.
///
/// Keyed by `(repo_slug, item_number)`. Reads return a shared reference;
/// callers that need ownership must `.clone()` the inner `data` field.
#[derive(Debug, Default)]
pub struct DetailCache {
    /// PR cache, keyed by `(repo_slug, number)`.
    // `pub(crate)` so tests in sibling modules can manufacture stale entries.
    pub(crate) prs: HashMap<(String, u32), Cached<PrDetail>>,
    /// Issue cache, keyed by `(repo_slug, number)`.
    pub(crate) issues: HashMap<(String, u32), Cached<IssueDetail>>,
}

impl DetailCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace the cached entry for a PR.
    ///
    /// The `fetched_at` timestamp is reset to `Instant::now()`.
    pub fn insert_pr(&mut self, detail: PrDetail) {
        let key = (detail.repo.clone(), detail.number);
        self.prs.insert(key, Cached::new(detail));
    }

    /// Insert or replace the cached entry for an issue.
    ///
    /// The `fetched_at` timestamp is reset to `Instant::now()`.
    pub fn insert_issue(&mut self, detail: IssueDetail) {
        let key = (detail.repo.clone(), detail.number);
        self.issues.insert(key, Cached::new(detail));
    }

    /// Look up a cached PR entry.
    ///
    /// Returns `None` on a cold miss.
    pub fn get_pr(&self, repo: &str, number: u32) -> Option<&Cached<PrDetail>> {
        self.prs.get(&(repo.to_owned(), number))
    }

    /// Look up a cached issue entry.
    ///
    /// Returns `None` on a cold miss.
    pub fn get_issue(&self, repo: &str, number: u32) -> Option<&Cached<IssueDetail>> {
        self.issues.get(&(repo.to_owned(), number))
    }

    /// Remove the cached PR entry for `(repo, number)`, if present.
    ///
    /// Used by the manual-refresh (`r`) path so the next fetch is treated as a
    /// cold miss and the spinner is shown.
    pub fn invalidate_pr(&mut self, repo: &str, number: u32) {
        self.prs.remove(&(repo.to_owned(), number));
    }

    /// Remove the cached issue entry for `(repo, number)`, if present.
    pub fn invalidate_issue(&mut self, repo: &str, number: u32) {
        self.issues.remove(&(repo.to_owned(), number));
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::detail::{IssueDetail, PrDetail};
    use chrono::Utc;

    fn make_pr_detail(repo: &str, number: u32) -> PrDetail {
        PrDetail {
            repo: repo.to_owned(),
            number,
            title: "Test PR".to_owned(),
            url: format!("https://github.com/{repo}/pull/{number}"),
            author: "user".to_owned(),
            body_markdown: String::new(),
            base_ref: "main".to_owned(),
            head_ref: "feat/x".to_owned(),
            is_draft: false,
            additions: 0,
            deletions: 0,
            changed_files_count: 0,
            updated_at: Utc::now(),
            created_at: Utc::now(),
            merged: false,
            files: vec![],
            check_runs: vec![],
            reviews: vec![],
            review_threads: vec![],
            issue_comments: vec![],
            commits: vec![],
        }
    }

    fn make_issue_detail(repo: &str, number: u32) -> IssueDetail {
        IssueDetail {
            repo: repo.to_owned(),
            number,
            title: "Test Issue".to_owned(),
            url: format!("https://github.com/{repo}/issues/{number}"),
            author: "user".to_owned(),
            body_markdown: String::new(),
            state: "OPEN".to_owned(),
            updated_at: Utc::now(),
            created_at: Utc::now(),
            labels: vec![],
            assignees: vec![],
            comments: vec![],
        }
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn cache_insert_and_get_pr() {
        let mut cache = DetailCache::new();
        let detail = make_pr_detail("o/r", 42);
        cache.insert_pr(detail.clone());

        let hit = cache.get_pr("o/r", 42).expect("should be a cache hit");
        assert_eq!(hit.data.number, 42);
        assert_eq!(hit.data.repo, "o/r");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn cache_insert_and_get_issue() {
        let mut cache = DetailCache::new();
        let detail = make_issue_detail("o/r", 7);
        cache.insert_issue(detail.clone());

        let hit = cache.get_issue("o/r", 7).expect("should be a cache hit");
        assert_eq!(hit.data.number, 7);
    }

    #[test]
    fn cache_is_fresh_true_under_ttl_false_after() {
        let data = make_pr_detail("o/r", 1);

        // Fresh entry: fetched_at = now.
        let fresh = Cached::new(data.clone());
        assert!(fresh.is_fresh(), "entry stamped now must be fresh");

        // Stale entry: fetched_at = 61 seconds ago. `checked_sub` avoids
        // the unchecked `Duration` subtraction clippy flags as a potential
        // panic on systems where `Instant::now()` is near process start.
        let stale = Cached {
            data,
            fetched_at: Instant::now()
                .checked_sub(Duration::from_secs(CACHE_TTL.as_secs() + 1))
                .unwrap_or_else(Instant::now),
        };
        assert!(!stale.is_fresh(), "entry older than TTL must be stale");
    }

    #[test]
    fn cache_invalidate_pr_removes_entry() {
        let mut cache = DetailCache::new();
        cache.insert_pr(make_pr_detail("o/r", 5));

        cache.invalidate_pr("o/r", 5);
        assert!(cache.get_pr("o/r", 5).is_none(), "invalidated entry must not be present");
    }

    #[test]
    fn cache_miss_on_unknown_key() {
        let cache = DetailCache::new();
        assert!(cache.get_pr("x/y", 999).is_none());
        assert!(cache.get_issue("x/y", 999).is_none());
    }
}
