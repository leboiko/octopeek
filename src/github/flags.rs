//! Action flags: single-value PR status classification for the UI.
//!
//! [`PullRequest::primary_flag`] collapses all PR state into one [`ActionFlag`]
//! following a strict priority tree so the UI always shows the most actionable
//! status.

use super::types::{CheckState, MergeStateStatus, Mergeable, PullRequest, ReviewDecision};

/// Single-value status classification for a pull request.
///
/// The priority tree is fixed and must not be reordered without updating the
/// corresponding unit tests and the Phase 3 UI spec.
// Used by the Phase 3 UI rendering layer; dead_code until that phase lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionFlag {
    /// PR is still in draft state.
    Draft,
    /// The branch has merge conflicts.
    Conflict,
    /// At least one CI check is failing or errored.
    CiFailing,
    /// At least one reviewer has requested changes.
    ChangesRequested,
    /// The viewer has been explicitly requested to review this PR.
    ReviewRequested,
    /// There are open, non-outdated review threads.
    UnresolvedThreads,
    /// The branch is behind the base branch.
    Behind,
    /// Everything looks good.
    Clean,
}

impl PullRequest {
    /// Collapse all PR state into a single [`ActionFlag`] using a fixed
    /// priority tree.  The first matching condition wins.
    ///
    /// # Arguments
    ///
    /// * `viewer_login` — GitHub login of the authenticated viewer, used to
    ///   check whether the viewer is among the requested reviewers.
    // Called from the Phase 3 UI list renderer; suppress dead_code until then.
    #[allow(dead_code)]
    pub fn primary_flag(&self, viewer_login: &str) -> ActionFlag {
        // 1. Draft is always shown first — the PR isn't ready for any action.
        if self.is_draft {
            return ActionFlag::Draft;
        }

        // 2. Merge conflicts block everything else.
        if self.mergeable == Mergeable::Conflicting {
            return ActionFlag::Conflict;
        }

        // 3. CI failures need to be fixed before the PR can land.
        if matches!(self.check_state, Some(CheckState::Failure | CheckState::Error)) {
            return ActionFlag::CiFailing;
        }

        // 4. A change-request from any reviewer blocks merging.
        if self.review_decision == Some(ReviewDecision::ChangesRequested) {
            return ActionFlag::ChangesRequested;
        }

        // 5. The viewer owes a review.
        if self.requested_reviewers.iter().any(|r| r == viewer_login) {
            return ActionFlag::ReviewRequested;
        }

        // 6. Open threads need to be resolved before merging.
        if self.unresolved_threads > 0 {
            return ActionFlag::UnresolvedThreads;
        }

        // 7. Branch needs to be rebased/merged with the base.
        if self.merge_state == MergeStateStatus::Behind {
            return ActionFlag::Behind;
        }

        // 8. Nothing is blocking — the PR is clean.
        ActionFlag::Clean
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::github::types::{CheckState, MergeStateStatus, Mergeable, ReviewDecision, Role};

    /// Build a "clean" PR and mutate specific fields per test case.
    fn base_pr() -> PullRequest {
        PullRequest {
            number: 1,
            title: "Test".to_owned(),
            url: "https://github.com/o/r/pull/1".to_owned(),
            repo: "o/r".to_owned(),
            author: "author".to_owned(),
            is_draft: false,
            mergeable: Mergeable::Mergeable,
            merge_state: MergeStateStatus::Clean,
            review_decision: None,
            commits_count: 1,
            comments_count: 0,
            check_state: Some(CheckState::Success),
            failing_checks: vec![],
            unresolved_threads: 0,
            requested_reviewers: vec![],
            reviews: vec![],
            updated_at: Utc::now(),
            roles: vec![Role::Author],
            base_ref: Some("main".to_owned()),
            head_ref: Some("feat/test".to_owned()),
        }
    }

    #[test]
    fn draft_wins_over_everything() {
        let mut pr = base_pr();
        pr.is_draft = true;
        // Pile on every lower-priority signal: a draft with conflicts AND CI
        // failure AND changes-requested must still report Draft.
        pr.mergeable = Mergeable::Conflicting;
        pr.check_state = Some(CheckState::Failure);
        pr.review_decision = Some(ReviewDecision::ChangesRequested);
        pr.requested_reviewers = vec!["viewer".to_owned()];
        pr.unresolved_threads = 5;
        pr.merge_state = MergeStateStatus::Behind;
        assert_eq!(pr.primary_flag("viewer"), ActionFlag::Draft);
    }

    #[test]
    fn conflict_after_draft() {
        let mut pr = base_pr();
        pr.mergeable = Mergeable::Conflicting;
        assert_eq!(pr.primary_flag("viewer"), ActionFlag::Conflict);
    }

    #[test]
    fn ci_failure_after_conflict() {
        let mut pr = base_pr();
        pr.check_state = Some(CheckState::Failure);
        assert_eq!(pr.primary_flag("viewer"), ActionFlag::CiFailing);
    }

    #[test]
    fn ci_error_triggers_ci_failing() {
        let mut pr = base_pr();
        pr.check_state = Some(CheckState::Error);
        assert_eq!(pr.primary_flag("viewer"), ActionFlag::CiFailing);
    }

    #[test]
    fn changes_requested_after_ci() {
        let mut pr = base_pr();
        pr.review_decision = Some(ReviewDecision::ChangesRequested);
        assert_eq!(pr.primary_flag("viewer"), ActionFlag::ChangesRequested);
    }

    #[test]
    fn review_requested_for_viewer() {
        let mut pr = base_pr();
        pr.requested_reviewers = vec!["viewer".to_owned()];
        assert_eq!(pr.primary_flag("viewer"), ActionFlag::ReviewRequested);
    }

    #[test]
    fn review_requested_not_for_viewer() {
        let mut pr = base_pr();
        pr.requested_reviewers = vec!["someone-else".to_owned()];
        assert_eq!(pr.primary_flag("viewer"), ActionFlag::Clean);
    }

    #[test]
    fn unresolved_threads() {
        let mut pr = base_pr();
        pr.unresolved_threads = 2;
        assert_eq!(pr.primary_flag("viewer"), ActionFlag::UnresolvedThreads);
    }

    #[test]
    fn behind_base_branch() {
        let mut pr = base_pr();
        pr.merge_state = MergeStateStatus::Behind;
        assert_eq!(pr.primary_flag("viewer"), ActionFlag::Behind);
    }

    #[test]
    fn clean_pr() {
        let pr = base_pr();
        assert_eq!(pr.primary_flag("viewer"), ActionFlag::Clean);
    }
}
