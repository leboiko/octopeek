//! Lookup table mapping file + line to the review threads anchored there.
//!
//! Built once per `PrDetail` load (see `App::on_pr_detail_loaded`) so the
//! Files section renderer can cheaply answer "does this file have threads?"
//! and "what thread(s) live at line N of this file?" per render frame.
//!
//! Three buckets mirror the three cases `ReviewThread.line` + `is_outdated`
//! can take:
//!
//! - **active line-anchored**: `line == Some(n)` and `!is_outdated` — these
//!   are the threads the diff renderer inserts inline at line `n`.
//! - **file-level**: `line == None` — threads anchored to the file as a
//!   whole (e.g. `subjectType = FILE` on github.com). Rendered at the
//!   bottom of the file's diff in a labelled block.
//! - **outdated / orphan**: `is_outdated == true` OR the line exists but
//!   does not appear in the current diff (force-push drift). Same
//!   bottom-of-file block as file-level.
//!
//! The common case — finding threads for the currently-rendered diff line
//! — is a `HashMap` lookup on `(path, line)`.

use std::collections::HashMap;

use crate::github::detail::{PrDetail, ReviewThread};

/// Indexed view over `PrDetail::review_threads` keyed by file + line.
#[derive(Debug, Default)]
pub(crate) struct ThreadIndex {
    /// Threads anchored to a specific line in the **current** diff
    /// (not outdated). Key: `(path, line)`.
    active_by_line: HashMap<(String, u32), Vec<usize>>,
    /// File-level threads (`line == None`) or outdated threads. Key: `path`.
    /// The two states are grouped because both render in the same
    /// bottom-of-file overflow block.
    overflow_by_file: HashMap<String, Vec<usize>>,
    /// Per-file active-thread count. Populated alongside `active_by_line`
    /// so the Files overview doesn't have to re-scan the threads.
    counts_per_file: HashMap<String, usize>,
    /// Per-file unresolved (and non-outdated) thread count. Drives the
    /// overview's warning-coloured indicator when any thread still needs
    /// attention on a file.
    unresolved_per_file: HashMap<String, usize>,
}

impl ThreadIndex {
    /// Build the index from a slice of threads. O(N) over threads.
    pub(crate) fn build(threads: &[ReviewThread]) -> Self {
        let mut index = Self::default();
        for (i, thread) in threads.iter().enumerate() {
            *index.counts_per_file.entry(thread.path.clone()).or_insert(0) += 1;
            if !thread.is_resolved && !thread.is_outdated {
                *index.unresolved_per_file.entry(thread.path.clone()).or_insert(0) += 1;
            }
            match (thread.is_outdated, thread.line) {
                (false, Some(ln)) => {
                    index.active_by_line.entry((thread.path.clone(), ln)).or_default().push(i);
                }
                _ => {
                    index.overflow_by_file.entry(thread.path.clone()).or_default().push(i);
                }
            }
        }
        index
    }

    /// Threads active on `(path, line)`, in insertion order.
    ///
    /// Used by the inline diff-expansion renderer in 0.1.8.
    pub(crate) fn active_at(&self, path: &str, line: u32) -> &[usize] {
        self.active_by_line.get(&(path.to_owned(), line)).map_or(&[], Vec::as_slice)
    }

    /// File-level and outdated threads for `path`, in insertion order.
    /// Rendered at the bottom of the file's diff view.
    pub(crate) fn overflow(&self, path: &str) -> &[usize] {
        self.overflow_by_file.get(path).map_or(&[], Vec::as_slice)
    }

    /// Total thread count for `path` (active + file-level + outdated).
    /// Used for the `N` badge in the Files overview.
    pub(crate) fn total_for(&self, path: &str) -> usize {
        self.counts_per_file.get(path).copied().unwrap_or(0)
    }

    /// Unresolved, non-outdated thread count for `path`. When non-zero,
    /// the overview colours the badge in `palette.warning` to signal
    /// "still needs attention."
    pub(crate) fn unresolved_for(&self, path: &str) -> usize {
        self.unresolved_per_file.get(path).copied().unwrap_or(0)
    }

    /// Total distinct files that have at least one thread of any kind.
    /// Used by tests.
    #[cfg(test)]
    pub(crate) fn distinct_files(&self) -> usize {
        self.counts_per_file.len()
    }
}

/// Convenience: build a `ThreadIndex` from a `PrDetail`.
pub(crate) fn build_for(detail: &PrDetail) -> ThreadIndex {
    ThreadIndex::build(&detail.review_threads)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn mk_thread(path: &str, line: Option<u32>, outdated: bool, resolved: bool) -> ReviewThread {
        ReviewThread {
            node_id: "THREAD_node".to_owned(),
            path: path.to_owned(),
            line,
            start_line: None,
            is_resolved: resolved,
            is_outdated: outdated,
            diff_hunk: None,
            comments: vec![crate::github::detail::ReviewComment {
                node_id: "COMMENT_node".to_owned(),
                author: "u".to_owned(),
                body_markdown: "c".to_owned(),
                created_at: Utc::now(),
                diff_hunk: None,
                original_commit_id: None,
            }],
        }
    }

    #[test]
    fn active_line_threads_are_indexed_per_file_and_line() {
        let threads = vec![
            mk_thread("src/a.rs", Some(10), false, false),
            mk_thread("src/a.rs", Some(20), false, false),
            mk_thread("src/b.rs", Some(5), false, false),
        ];
        let idx = ThreadIndex::build(&threads);

        assert_eq!(idx.active_at("src/a.rs", 10).len(), 1);
        assert_eq!(idx.active_at("src/a.rs", 20).len(), 1);
        assert_eq!(idx.active_at("src/b.rs", 5).len(), 1);
        assert_eq!(idx.active_at("src/a.rs", 99).len(), 0, "unrelated line returns empty");
        assert_eq!(idx.total_for("src/a.rs"), 2);
        assert_eq!(idx.total_for("src/b.rs"), 1);
        assert_eq!(idx.unresolved_for("src/a.rs"), 2);
        assert_eq!(idx.distinct_files(), 2);
    }

    #[test]
    fn outdated_and_file_level_go_to_overflow() {
        let threads = vec![
            mk_thread("src/a.rs", None, false, false),    // file-level
            mk_thread("src/a.rs", Some(10), true, false), // outdated — line coord ignored
        ];
        let idx = ThreadIndex::build(&threads);

        assert_eq!(idx.active_at("src/a.rs", 10).len(), 0, "outdated must NOT show at line 10");
        assert_eq!(
            idx.overflow("src/a.rs").len(),
            2,
            "both file-level and outdated go to overflow"
        );
        assert_eq!(idx.total_for("src/a.rs"), 2);
        // File-level is not "unresolved" in the attention sense because it's
        // not anchored to diff line; outdated is excluded from unresolved too.
        // But the threads themselves are unresolved (`is_resolved == false`),
        // so unresolved_per_file counts them — the UI layer decides whether
        // to treat overflow threads as call-to-action.
        assert_eq!(
            idx.unresolved_for("src/a.rs"),
            1,
            "file-level unresolved counts; outdated doesn't"
        );
    }

    #[test]
    fn resolved_thread_does_not_count_as_unresolved() {
        let threads = vec![
            mk_thread("src/a.rs", Some(1), false, true),  // resolved
            mk_thread("src/a.rs", Some(2), false, false), // unresolved
        ];
        let idx = ThreadIndex::build(&threads);
        assert_eq!(idx.total_for("src/a.rs"), 2);
        assert_eq!(idx.unresolved_for("src/a.rs"), 1);
    }

    #[test]
    fn empty_threads_produce_empty_index() {
        let idx = ThreadIndex::build(&[]);
        assert_eq!(idx.total_for("anywhere"), 0);
        assert_eq!(idx.unresolved_for("anywhere"), 0);
        assert_eq!(idx.active_at("anywhere", 1).len(), 0);
        assert_eq!(idx.overflow("anywhere").len(), 0);
    }
}
