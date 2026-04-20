//! `handle_action` dispatcher and per-action handler helpers.

use tracing::{debug, warn};

use crate::ui::pr_detail::DetailSection;

use super::actions::Action;
use super::state::App;
use super::types::{DetailKind, Focus, PerTabState};

impl App {
    /// Route an action to the appropriate handler.
    // Taking `Action` by value is correct — the dispatcher owns and consumes
    // the action. clippy prefers `&Action` here but that would require cloning
    // for variants like `RawKey(KeyEvent)` which are not `Copy`.
    #[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
    pub(super) fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.running = false;
            }
            Action::RawKey(key) => {
                self.handle_key(key);
            }
            Action::Resize(w, h) => {
                // ratatui redraws the full frame on the next loop iteration;
                // no explicit handling is required for resize events.
                debug!("terminal resized to {w}x{h}");
            }
            Action::Mouse(m) => {
                self.handle_mouse(m);
            }
            Action::NextTab => {
                self.save_current_tab_state();
                self.tabs.next();
                self.restore_active_tab_state();
            }
            Action::PrevTab => {
                self.save_current_tab_state();
                self.tabs.prev();
                self.restore_active_tab_state();
            }
            Action::SwitchTab(idx) => {
                self.save_current_tab_state();
                self.tabs.set_active_by_index(idx);
                self.restore_active_tab_state();
            }
            Action::OpenHelp => {
                self.show_help = !self.show_help;
                if self.show_help {
                    self.focus = Focus::Help;
                } else {
                    self.focus = Focus::Dashboard;
                }
            }
            Action::Refresh | Action::RefreshAll => {
                if let Some(tx) = self.action_tx.clone() {
                    self.spawn_fetch(tx);
                }
            }
            Action::ToggleShowAll => {
                self.config.show_all_prs = !self.config.show_all_prs;
                self.config.save();
                let msg = if self.config.show_all_prs {
                    "Showing all open PRs/issues"
                } else {
                    "Showing only yours"
                };
                self.show_flash(msg, std::time::Duration::from_secs(3));
                // Kick a fresh fetch with the new mode.
                if let Some(tx) = self.action_tx.clone() {
                    self.spawn_fetch(tx);
                }
            }
            Action::OpenDetail => {
                warn!("Action::OpenDetail not yet implemented (Phase 3)");
            }
            Action::BackToDashboard => {
                warn!("Action::BackToDashboard not yet implemented (Phase 3)");
            }
            Action::ToggleView => {
                if let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) {
                    let current = self.session.view_mode(&repo);
                    let next = match current {
                        crate::state::ViewMode::Prs => crate::state::ViewMode::Issues,
                        crate::state::ViewMode::Issues => crate::state::ViewMode::Prs,
                    };
                    self.session.set_view_mode(&repo, next);
                    // Reset selection to 0 when toggling view.
                    self.selection.insert(repo, 0);
                }
            }
            Action::OpenInBrowser => {
                warn!("Action::OpenInBrowser not yet implemented (Phase 4)");
            }
            Action::CopyUrl => {
                warn!("Action::CopyUrl not yet implemented (Phase 4)");
            }
            Action::CheckoutBranch => {
                self.begin_checkout_from_selection();
            }
            Action::ConfirmCheckout(confirmed) => {
                if confirmed {
                    self.execute_confirm();
                } else {
                    self.dismiss_confirm();
                }
            }
            Action::OpenRepoPicker => {
                debug!("Action::OpenRepoPicker: switching focus from {:?}", self.focus);
                self.repo_picker_list_cursor = 0;
                self.repo_picker_input.clear();
                self.repo_picker_mode = super::types::RepoPickerMode::List;
                self.repo_picker_return_focus = self.focus;
                self.focus = Focus::RepoPicker;
            }
            Action::InboxFetchStarted => {
                self.fetching = true;
            }
            Action::InboxLoaded(inbox) => {
                self.on_inbox_loaded(*inbox);
            }
            Action::FetchFailed(msg) => {
                self.on_fetch_failed(msg);
            }
            Action::FetchPrDetail(repo, number) => {
                if let Some(tx) = self.action_tx.clone() {
                    self.spawn_detail_fetch(DetailKind::Pr, repo, number, tx);
                }
            }
            Action::FetchIssueDetail(repo, number) => {
                if let Some(tx) = self.action_tx.clone() {
                    self.spawn_detail_fetch(DetailKind::Issue, repo, number, tx);
                }
            }
            Action::PrDetailLoaded(detail) => {
                // Always upsert into cache — background SWR fetches land here
                // too, even if the user has tabbed away.
                self.detail_cache.insert_pr(*detail.clone());

                // Only update visible state when the user is still looking at
                // this exact item. Silently dropping is correct: the cache
                // already holds the fresh data for the next visit.
                let active_pr_matches = self
                    .pr_detail
                    .as_ref()
                    .is_some_and(|d| d.repo == detail.repo && d.number == detail.number);
                // Also accept when pr_detail is None but focus is Detail and
                // this was a foreground (cold-miss) fetch.
                let foreground_cold_miss =
                    self.pr_detail.is_none() && self.focus == Focus::Detail && self.detail_fetching;
                if self.focus == Focus::Detail && (active_pr_matches || foreground_cold_miss) {
                    let pr_detail = *detail.clone();
                    // Rebuild the thread index alongside every new PrDetail so
                    // the Files overview + future inline-expansion path have
                    // O(1) `(path, line)` lookups on the current thread set.
                    self.thread_index = Some(crate::ui::pr_detail::build_thread_index(&pr_detail));
                    self.pr_detail = Some(pr_detail);
                }
                self.clear_detail_loading_markers(&detail.repo, detail.number);
            }
            Action::IssueDetailLoaded(detail) => {
                self.detail_cache.insert_issue(*detail.clone());

                let active_issue_matches = self
                    .issue_detail
                    .as_ref()
                    .is_some_and(|d| d.repo == detail.repo && d.number == detail.number);
                let foreground_cold_miss = self.issue_detail.is_none()
                    && self.focus == Focus::Detail
                    && self.detail_fetching;
                if self.focus == Focus::Detail && (active_issue_matches || foreground_cold_miss) {
                    self.issue_detail = Some(*detail.clone());
                }
                self.clear_detail_loading_markers(&detail.repo, detail.number);
            }
            Action::DetailFetchFailed(msg) => {
                self.detail_fetching = false;
                // A failed background SWR fetch must not erase the stale content
                // the user is currently reading — only log the warning.
                warn!("GitHub detail fetch failed: {msg}");
                // Only surface the error when no stale content is available.
                if self.pr_detail.is_none() && self.issue_detail.is_none() {
                    self.detail_error = Some(msg);
                }
                // Clear the SWR marker regardless so a future tick can retry.
                self.detail_refreshing = None;
            }
            Action::AutoRefresh => {
                // Inbox refresh (same as RefreshAll / Refresh).
                if let Some(tx) = self.action_tx.clone() {
                    self.spawn_fetch(tx.clone());

                    // Detail SWR: re-fetch the open item if one exists and we
                    // are not already re-fetching it.
                    if self.focus == Focus::Detail {
                        let detail_key: Option<(DetailKind, String, u32)> =
                            if let Some(d) = &self.pr_detail {
                                Some((DetailKind::Pr, d.repo.clone(), d.number))
                            } else {
                                self.issue_detail
                                    .as_ref()
                                    .map(|d| (DetailKind::Issue, d.repo.clone(), d.number))
                            };

                        if let Some((kind, repo, number)) = detail_key {
                            let already = self
                                .detail_refreshing
                                .as_ref()
                                .is_some_and(|(r, n)| r == &repo && *n == number);
                            if !already {
                                self.detail_refreshing = Some((repo.clone(), number));
                                self.spawn_detail_fetch_background(kind, repo, number, tx);
                            }
                        }
                    }
                }
            }
        }
        // Every action path could have mutated `pr_detail_scroll` — explicitly
        // in the scroll keys, implicitly in focus transitions or new data
        // arriving. A single clamp here guarantees the offset never points
        // past the current content's end (which previously left users staring
        // at a blank frame and punching `k` to recover).
        self.clamp_pr_detail_scroll();
    }

    /// Return to the dashboard, clearing all detail state.
    ///
    /// Also clears the per-tab detail ref for the active tab — an explicit
    /// Esc / `b` means "I'm done with that PR", so a later tab round-trip
    /// should land on the dashboard list, not auto-reopen what we just left.
    pub(super) fn back_to_dashboard(&mut self) {
        if let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) {
            self.per_tab_state.insert(repo, PerTabState::default());
        }
        self.focus = Focus::Dashboard;
        self.pr_detail = None;
        self.issue_detail = None;
        self.detail_error = None;
        self.detail_fetching = false;
        // NOTE: `detail_cache` is intentionally NOT cleared here. The cached
        // payloads remain available for instant serving on the next visit.
        self.detail_refreshing = None;
        self.pr_detail_scroll.clear();
        self.pr_detail_diff_scroll.clear();
        self.pr_detail_files_expanded = false;
        self.detail_comments_expanded = false;
        self.pr_detail_selected_section = DetailSection::default();
        self.pr_detail_files_cursor = 0;
        self.pr_detail_files_show_diff = false;
        self.pr_detail_sidebar_scroll = 0;
        self.pr_detail_expanded_threads.clear();
        *self.pr_detail_diff_cursor.borrow_mut() = None;
        self.copy_mode.exit();
    }

    /// Open the detail view for the currently selected PR or issue.
    ///
    /// Reads the active tab, view mode, and selection index to determine which
    /// item to fetch, then dispatches the appropriate detail action and switches
    /// focus to [`Focus::Detail`].
    pub(super) fn open_detail_for_selection(&mut self) {
        let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) else {
            return;
        };
        let Some(inbox) = &self.inbox else {
            return;
        };

        let mode = self.session.view_mode(&repo);
        let sel = self.selection.get(&repo).copied().unwrap_or(0);

        match mode {
            crate::state::ViewMode::Prs => {
                let prs = crate::github::types::sorted_prs_for_repo(inbox, &repo);
                if let Some(pr) = prs.get(sel) {
                    let number = pr.number;
                    // Reset detail state before switching focus. `detail_fetching`
                    // is set by `spawn_detail_fetch` itself — if we set it here
                    // too, the guard inside that function sees `true` and
                    // silently skips the fetch, leaving the view stuck on the
                    // spinner forever.
                    self.pr_detail = None;
                    self.issue_detail = None;
                    self.detail_error = None;
                    self.pr_detail_scroll.clear();
                    self.pr_detail_diff_scroll.clear();
                    self.pr_detail_files_expanded = false;
                    self.detail_comments_expanded = false;
                    self.pr_detail_selected_section = DetailSection::default();
                    self.pr_detail_files_cursor = 0;
                    self.pr_detail_sidebar_scroll = 0;
                    self.focus = Focus::Detail;
                    if let Some(tx) = self.action_tx.clone() {
                        self.spawn_detail_fetch(DetailKind::Pr, repo, number, tx);
                    }
                }
            }
            crate::state::ViewMode::Issues => {
                let issues = crate::github::types::sorted_issues_for_repo(inbox, &repo);
                if let Some(issue) = issues.get(sel) {
                    let number = issue.number;
                    // See the PR branch above: `detail_fetching` is set by
                    // `spawn_detail_fetch`, not here, to avoid the self-
                    // blocking guard.
                    self.pr_detail = None;
                    self.issue_detail = None;
                    self.detail_error = None;
                    self.pr_detail_scroll.clear();
                    self.pr_detail_diff_scroll.clear();
                    self.pr_detail_files_expanded = false;
                    self.detail_comments_expanded = false;
                    self.pr_detail_selected_section = DetailSection::default();
                    self.pr_detail_files_cursor = 0;
                    self.pr_detail_sidebar_scroll = 0;
                    self.focus = Focus::Detail;
                    if let Some(tx) = self.action_tx.clone() {
                        self.spawn_detail_fetch(DetailKind::Issue, repo, number, tx);
                    }
                }
            }
        }
    }

    // ── Confirmation overlay ─────────────────────────────────────────────────

    /// Dismiss the confirmation overlay and restore prior focus.
    pub(super) fn dismiss_confirm(&mut self) {
        self.confirm = None;
        self.focus = self.confirm_return_focus;
    }

    /// Execute the pending confirmation action, then dismiss the overlay.
    pub(super) fn execute_confirm(&mut self) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        self.focus = self.confirm_return_focus;

        match confirm.pending_action {
            crate::ui::confirm::ConfirmPending::CheckoutBranch { branch, .. } => {
                // Check tree cleanliness before running git.
                match crate::git::is_working_tree_clean() {
                    Err(e) => {
                        self.show_flash(
                            format!("git checkout failed: {e}"),
                            std::time::Duration::from_secs(4),
                        );
                        return;
                    }
                    Ok(false) => {
                        self.show_flash(
                            "Working tree is not clean; commit or stash first.",
                            std::time::Duration::from_secs(4),
                        );
                        return;
                    }
                    Ok(true) => {}
                }

                match crate::git::checkout_branch(&branch) {
                    Ok(()) => {
                        self.show_flash(
                            format!("Checked out {branch}"),
                            std::time::Duration::from_secs(3),
                        );
                    }
                    Err(e) => {
                        self.show_flash(
                            format!("git checkout failed: {e}"),
                            std::time::Duration::from_secs(4),
                        );
                    }
                }
            }
        }
    }

    /// Build a `Confirm` overlay for checking out the selected PR's head branch.
    ///
    /// Reads from `pr_detail` when in Detail focus, or from the list-level
    /// `PullRequest` when in Dashboard focus.  No-ops if no branch can be
    /// resolved.
    pub(super) fn begin_checkout_from_selection(&mut self) {
        // Prefer the detail view's head_ref when we are in it.
        let (branch, repo, number) = if let Some(detail) = &self.pr_detail {
            // `PrDetail::head_ref` is a `String`, so it can technically be
            // empty even though GitHub would not return one. Guard against it
            // so we never shell out `git checkout ""` (which produces a
            // confusing usage error instead of a clean flash).
            if detail.head_ref.is_empty() {
                self.show_flash(
                    "Branch info not available for this PR.",
                    std::time::Duration::from_secs(3),
                );
                return;
            }
            (detail.head_ref.clone(), detail.repo.clone(), detail.number)
        } else {
            // Fall back to the list-level PullRequest.
            let Some(repo_slug) = self.tabs.active_tab().map(|t| t.repo.clone()) else {
                return;
            };
            let Some(inbox) = &self.inbox else {
                return;
            };
            let sel = self.selection.get(&repo_slug).copied().unwrap_or(0);
            let prs = crate::github::types::sorted_prs_for_repo(inbox, &repo_slug);
            let Some(pr) = prs.get(sel) else {
                return;
            };
            let Some(head) = pr.head_ref.clone() else {
                self.show_flash(
                    "Branch info not available; open the detail view first.",
                    std::time::Duration::from_secs(3),
                );
                return;
            };
            (head, repo_slug, pr.number)
        };

        // Warn when not in a git repo.
        if !crate::git::repo_cwd_is_git() {
            self.show_flash(
                "Not in a git repository; cannot checkout branch.",
                std::time::Duration::from_secs(3),
            );
            return;
        }

        let cwd =
            std::env::current_dir().map_or_else(|_| ".".to_owned(), |p| p.display().to_string());

        let confirm = crate::ui::confirm::Confirm {
            title: "Checkout branch".to_owned(),
            prompt: format!("Checkout `{branch}` in {cwd}?"),
            pending_action: crate::ui::confirm::ConfirmPending::CheckoutBranch {
                repo: repo.clone(),
                number,
                branch,
            },
        };

        self.confirm = Some(confirm);
        self.confirm_return_focus = self.focus;
        self.focus = Focus::Confirm;
    }
}
