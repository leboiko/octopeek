//! Background fetch helpers: inbox, detail, SWR kicks, and scroll clamping.

use std::collections::HashMap;
use tracing::{debug, warn};

use crate::github;

use super::actions::Action;
use super::state::App;
use super::types::{DetailKind, DetailRef, FirstRunSuggestion, Focus, PerTabState};

impl App {
    /// Spawn a background task that fetches the inbox and sends the result back
    /// via the action channel.  Guards against concurrent fetches via `fetching`.
    pub(super) fn spawn_fetch(&mut self, tx: tokio::sync::mpsc::UnboundedSender<Action>) {
        if self.fetching {
            debug!("fetch already in progress; skipping");
            return;
        }
        let Some(client) = self.client.clone() else {
            debug!("no GitHub client; skipping fetch");
            return;
        };

        self.fetching = true;
        send_or_warn(&tx, Action::InboxFetchStarted);

        // Spawn a supervisor task that awaits an inner task's JoinHandle. If
        // the inner task panics, `JoinHandle::await` returns an `Err(JoinError)`
        // with `is_panic() == true`, so we can always emit a terminal action —
        // neither `InboxLoaded` nor `FetchFailed` must ever be skipped, or the
        // `fetching` guard would pin itself on `true` forever.
        // Clone what we need before moving into the async block.
        let repos = self.config.repos.clone();
        let show_all = self.config.show_all_prs;

        tokio::spawn(async move {
            let inner = tokio::spawn(async move {
                if show_all {
                    client.fetch_inbox_all(&repos).await
                } else {
                    client.fetch_inbox().await
                }
            });
            let action = match inner.await {
                Ok(Ok(inbox)) => Action::InboxLoaded(Box::new(inbox)),
                Ok(Err(e)) => Action::FetchFailed(e.to_string()),
                Err(join_err) if join_err.is_panic() => {
                    Action::FetchFailed(format!("fetch task panicked: {join_err}"))
                }
                Err(join_err) => Action::FetchFailed(format!("fetch task aborted: {join_err}")),
            };
            send_or_warn(&tx, action);
        });
    }

    /// Handle a successfully fetched inbox: store data, update tab badges, clear error state.
    pub(super) fn on_inbox_loaded(&mut self, inbox: github::Inbox) {
        let viewer_login = inbox.viewer_login.clone();

        // Update each tab's needs_action_count from the new inbox. Every open
        // issue assigned to the viewer is counted (the assignment query
        // `assignee:@me` already limits the set to items the viewer is
        // responsible for), while PRs are filtered by primary_flag to exclude
        // Clean and Draft states.
        for tab in &mut self.tabs.tabs {
            let count = inbox
                .prs
                .iter()
                .filter(|pr| pr.repo == tab.repo)
                .filter(|pr| {
                    let flag = pr.primary_flag(&viewer_login);
                    flag != crate::github::flags::ActionFlag::Clean
                        && flag != crate::github::flags::ActionFlag::Draft
                })
                .count()
                + inbox.issues.iter().filter(|i| i.repo == tab.repo).count();
            tab.needs_action_count = Some(count);
        }

        // Clamp any stale per-repo selection indices so they cannot point past
        // the end of the refreshed list (a blocking render bug if a repo
        // shrinks between refreshes). `draw_*_list` clamps defensively too,
        // but keeping the canonical state consistent avoids subtle surprises
        // elsewhere (e.g. the Phase 4 detail view will key off this index).
        for (repo, idx) in &mut self.selection {
            let max_pr = inbox.prs.iter().filter(|pr| pr.repo == *repo).count();
            let max_issue = inbox.issues.iter().filter(|i| i.repo == *repo).count();
            let max = max_pr.max(max_issue);
            if max == 0 {
                *idx = 0;
            } else if *idx >= max {
                *idx = max - 1;
            }
        }

        // First-run wizard: when config is still empty and focus is on the
        // dashboard (not an overlay or detail), compute repo suggestions from
        // the inbox and switch to the wizard focus.
        if self.config.repos.is_empty() && self.focus == Focus::Dashboard {
            let mut counts: HashMap<String, usize> = HashMap::new();
            for pr in &inbox.prs {
                *counts.entry(pr.repo.clone()).or_insert(0) += 1;
            }
            for issue in &inbox.issues {
                *counts.entry(issue.repo.clone()).or_insert(0) += 1;
            }
            if !counts.is_empty() {
                // Sort by count descending, then alphabetically by repo for
                // stable ordering when counts are equal.
                let mut suggestions: Vec<FirstRunSuggestion> = counts
                    .into_iter()
                    .map(|(repo, count)| FirstRunSuggestion { repo, count, selected: false })
                    .collect();
                suggestions.sort_unstable_by(|a, b| {
                    b.count.cmp(&a.count).then_with(|| a.repo.cmp(&b.repo))
                });
                self.first_run_suggestions = suggestions;
                self.first_run_cursor = 0;
                self.focus = Focus::FirstRun;
            }
        }

        self.inbox = Some(inbox);
        self.inbox_loaded_at = Some(chrono::Utc::now());
        self.fetching = false;
        self.last_fetch_error = None;
    }

    /// Handle a failed fetch: record the error, keep any cached inbox.
    pub(super) fn on_fetch_failed(&mut self, err: String) {
        self.fetching = false;
        warn!("GitHub inbox fetch failed: {err}");
        self.last_fetch_error = Some(err);
    }

    /// Spawn a background task that fetches PR or issue detail and sends the
    /// result back via the action channel.
    ///
    /// Guards against concurrent detail fetches via `detail_fetching`. Uses the
    /// same supervisor-task panic-catching pattern as [`Self::spawn_fetch`] so
    /// `detail_fetching` is always reset, even if the inner task panics.
    pub fn spawn_detail_fetch(
        &mut self,
        kind: DetailKind,
        repo: String,
        number: u32,
        tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) {
        if self.detail_fetching {
            debug!("detail fetch already in progress; skipping");
            return;
        }
        let Some(client) = self.client.clone() else {
            debug!("no GitHub client; skipping detail fetch");
            send_or_warn(&tx, Action::DetailFetchFailed("no GitHub client configured".to_owned()));
            return;
        };

        self.detail_fetching = true;
        spawn_supervised_detail_fetch(client, kind, repo, number, tx, "detail fetch");
    }

    /// Background variant of [`Self::spawn_detail_fetch`] used for
    /// stale-while-revalidate kicks and auto-refresh.
    ///
    /// Identical to `spawn_detail_fetch` **except** it does NOT set
    /// `detail_fetching = true`, so no spinner appears — the user already
    /// sees stale content while the fresh payload arrives silently.
    ///
    /// Returns `false` and is a no-op when no client is available.
    pub fn spawn_detail_fetch_background(
        &self,
        kind: DetailKind,
        repo: String,
        number: u32,
        tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> bool {
        let Some(client) = self.client.clone() else {
            debug!("no GitHub client; skipping background detail fetch");
            return false;
        };

        // Intentionally no `detail_fetching` guard: background SWR fetches are
        // allowed to run even while a foreground fetch is in progress (the
        // arriving action handler checks the active tab before overwriting state).
        spawn_supervised_detail_fetch(client, kind, repo, number, tx, "bg detail fetch");
        true
    }

    /// Return the filtered + sorted PR list length for the active repo.
    pub(super) fn active_list_len(&self) -> usize {
        let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) else {
            return 0;
        };
        let Some(inbox) = &self.inbox else {
            return 0;
        };
        let mode = self.session.view_mode(&repo);
        match mode {
            crate::state::ViewMode::Prs => inbox.prs.iter().filter(|p| p.repo == repo).count(),
            crate::state::ViewMode::Issues => {
                inbox.issues.iter().filter(|i| i.repo == repo).count()
            }
        }
    }

    /// Rebuild the rendered line buffer for the currently selected detail section.
    ///
    /// Copy-mode cursor motion and selection extraction work against the exact
    /// same `Vec<Line>` the renderer produces, so we call the same per-section
    /// builder here. Returns an empty `Vec` when no detail is loaded.
    pub(super) fn current_detail_lines(&self) -> Vec<ratatui::text::Line<'static>> {
        if let Some(detail) = &self.pr_detail {
            let (lines, _) = crate::ui::pr_detail::build_section(
                self.pr_detail_selected_section,
                detail,
                self.pr_detail_files_cursor,
                self.pr_detail_files_show_diff,
                self.detail_comments_expanded,
                &self.palette,
                self.config.show_ascii_glyphs,
            );
            return lines;
        }
        if let Some(detail) = &self.issue_detail {
            let (lines, _) = crate::ui::issue_detail::build_content(
                detail,
                self.detail_comments_expanded,
                &self.palette,
                self.config.show_ascii_glyphs,
            );
            return lines;
        }
        Vec::new()
    }

    /// Clamp the active section's scroll offset so it can never exceed
    /// `rendered_rows - viewport_height`.
    ///
    /// Without this, `G`, `d`, or the scroll wheel past the last line leaves
    /// the scroll counter pointing into the void — the renderer shows a blank
    /// screen and the user has to press `k` many times to recover.
    ///
    /// The row count must be the **wrapped** (rendered) row count, not the
    /// input line count — `ratatui::widgets::Paragraph` with `Wrap` expands
    /// long lines into multiple rows, and clamping against the unwrapped
    /// length leaves the tail of a wrapped comment unreachable. We build a
    /// throwaway `Paragraph` at the current viewport width and ask for
    /// `line_count`, which walks the same word-wrapper the renderer uses.
    pub(super) fn clamp_pr_detail_scroll(&mut self) {
        if !matches!(self.focus, Focus::Detail) {
            return;
        }
        let area = self.pr_detail_right_viewport.get();
        if area.height == 0 || area.width == 0 {
            return;
        }
        let lines = self.current_detail_lines();
        // Mirror the renderer's wrap decision: prose sections
        // (Description / Checks / Reviews / Comments) wrap, so count the
        // wrapped rows; the Files section does NOT wrap (see the comment
        // in `pr_detail::draw`), so its rendered row count is just
        // `lines.len()`. Counting wrapped rows for a non-wrapping section
        // would over-estimate and leave a big empty tail when the user
        // scrolls to the bottom of a diff.
        let wraps = self.pr_detail_selected_section != crate::ui::pr_detail::DetailSection::Files;
        let rendered_rows = if wraps {
            let probe = ratatui::widgets::Paragraph::new(lines)
                .wrap(ratatui::widgets::Wrap { trim: false });
            u16::try_from(probe.line_count(area.width)).unwrap_or(u16::MAX)
        } else {
            u16::try_from(lines.len()).unwrap_or(u16::MAX)
        };
        let max_scroll = rendered_rows.saturating_sub(area.height);
        // Route through `right_pane_scroll_mut` so we clamp whichever map
        // currently owns the right-pane scroll — the per-section map for
        // Description/Checks/Reviews/Comments, or the per-file diff map for
        // Files. Without this, scrolling inside a diff grew unbounded
        // because the clamp was operating on a different key entirely.
        let scroll = self.right_pane_scroll_mut();
        if *scroll > max_scroll {
            *scroll = max_scroll;
        }
    }

    /// Adjust the active section's scroll offset and `copy_mode.h_scroll` so
    /// that the cursor is always visible within the last-rendered viewport.
    pub(super) fn ensure_cursor_visible(&mut self, lines: &[ratatui::text::Line<'static>]) {
        let area = self.pr_detail_right_viewport.get();
        let (vw, vh) = (area.width, area.height);
        if vh == 0 {
            return;
        }
        let cursor_row = u16::try_from(self.copy_mode.cursor.row).unwrap_or(u16::MAX);
        let section = self.pr_detail_selected_section;
        {
            let scroll = self.scroll_mut(section);
            if cursor_row < *scroll {
                *scroll = cursor_row;
            } else if cursor_row >= scroll.saturating_add(vh) {
                *scroll = cursor_row.saturating_sub(vh).saturating_add(1);
            }
        }

        if vw == 0 {
            return;
        }
        let line = lines.get(self.copy_mode.cursor.row);
        let cursor_col = line
            .map_or(0, |l| crate::ui::copy_mode::cursor_display_col(l, self.copy_mode.cursor.col));
        if cursor_col < self.copy_mode.h_scroll {
            self.copy_mode.h_scroll = cursor_col;
        } else if cursor_col >= self.copy_mode.h_scroll.saturating_add(vw) {
            self.copy_mode.h_scroll = cursor_col.saturating_sub(vw).saturating_add(1);
        }
    }

    /// Record what the user is currently looking at in the active tab, so
    /// switching away and coming back can land them on the same detail view
    /// they left.
    ///
    /// Only the `(repo, number, kind)` reference is stored — the actual
    /// payload is dropped and re-fetched on restore. That costs one GraphQL
    /// round-trip (surfacing the existing "Fetching pull request…" spinner)
    /// but keeps the data fresh: check runs move, reviews arrive, comments
    /// land, all between a tab round-trip.
    pub(super) fn save_current_tab_state(&mut self) {
        let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) else {
            return;
        };

        let detail_ref = if self.focus == Focus::Detail {
            if let Some(d) = &self.pr_detail {
                Some(DetailRef { repo: d.repo.clone(), number: d.number, kind: DetailKind::Pr })
            } else {
                self.issue_detail.as_ref().map(|d| DetailRef {
                    repo: d.repo.clone(),
                    number: d.number,
                    kind: DetailKind::Issue,
                })
            }
        } else {
            None
        };

        self.per_tab_state.insert(repo, PerTabState { detail_ref });
    }

    /// Shared tail logic for `PrDetailLoaded` / `IssueDetailLoaded`:
    /// clear the SWR-in-flight marker when the arriving fetch matches, then
    /// mark loading as complete regardless.
    ///
    /// # Arguments
    ///
    /// * `repo`   - Repository slug of the arriving detail.
    /// * `number` - Issue/PR number of the arriving detail.
    pub(super) fn clear_detail_loading_markers(&mut self, repo: &str, number: u32) {
        if self.detail_refreshing.as_ref().is_some_and(|(r, n)| r == repo && *n == number) {
            self.detail_refreshing = None;
        }
        self.detail_fetching = false;
        self.detail_error = None;
    }

    /// Serve a single kind of detail from cache (with SWR kick if stale) or
    /// dispatch a foreground cold fetch.
    ///
    /// Called from `restore_active_tab_state` for both `DetailKind::Pr` and
    /// `DetailKind::Issue` to eliminate the parallel match arms.
    ///
    /// # Arguments
    ///
    /// * `kind`   - Whether to restore a PR or issue detail.
    /// * `repo`   - Repository slug.
    /// * `number` - PR/issue number.
    pub(super) fn restore_detail_kind(&mut self, kind: DetailKind, repo: String, number: u32) {
        // The kind-specific part: look up the cache, copy the cached data into
        // the matching detail field, and report freshness. `None` means a
        // cache miss; `Some(true)` fresh; `Some(false)` stale.
        let is_fresh: Option<bool> = match kind {
            DetailKind::Pr => self.detail_cache.get_pr(&repo, number).map(|c| {
                let fresh = c.is_fresh();
                let data = c.data.clone();
                self.pr_detail = Some(data);
                fresh
            }),
            DetailKind::Issue => self.detail_cache.get_issue(&repo, number).map(|c| {
                let fresh = c.is_fresh();
                let data = c.data.clone();
                self.issue_detail = Some(data);
                fresh
            }),
        };

        // The shared part: SWR flow is identical for PR and issue.
        match is_fresh {
            None => {
                if let Some(tx) = self.action_tx.clone() {
                    self.spawn_detail_fetch(kind, repo, number, tx);
                }
            }
            Some(true) => {} // cache hit + fresh: nothing more to do
            Some(false) => {
                let already_refreshing = self
                    .detail_refreshing
                    .as_ref()
                    .is_some_and(|(r, n)| r == &repo && *n == number);
                if !already_refreshing {
                    self.detail_refreshing = Some((repo.clone(), number));
                    if let Some(tx) = self.action_tx.clone() {
                        self.spawn_detail_fetch_background(kind, repo, number, tx);
                    }
                }
            }
        }
    }

    /// Restore the saved per-tab state for the active tab, clearing stale
    /// detail payload and either serving from cache or dispatching a cold fetch.
    ///
    /// **Stale-while-revalidate** behaviour:
    /// - Cache hit, fresh → show immediately, no background fetch.
    /// - Cache hit, stale → show immediately AND kick a background re-fetch.
    /// - Cache miss (cold) → show spinner, dispatch foreground fetch.
    ///
    /// Called right after `tabs.next()` / `tabs.prev()` / `set_active_by_index`.
    /// Falls back to `Focus::Dashboard` when no saved detail ref exists.
    pub(super) fn restore_active_tab_state(&mut self) {
        // Always clear whatever detail was loaded for the previous tab —
        // the renderer should never show that tab's data under a different
        // repo's header.
        self.pr_detail = None;
        self.issue_detail = None;
        self.detail_error = None;
        self.detail_fetching = false;

        let Some(repo) = self.tabs.active_tab().map(|t| t.repo.clone()) else {
            self.focus = Focus::Dashboard;
            return;
        };

        let saved = self.per_tab_state.get(&repo).cloned().unwrap_or_default();
        let Some(detail_ref) = saved.detail_ref else {
            self.focus = Focus::Dashboard;
            return;
        };

        self.focus = Focus::Detail;

        let dref_repo = detail_ref.repo.clone();
        let dref_number = detail_ref.number;

        self.restore_detail_kind(detail_ref.kind, dref_repo, dref_number);
    }

    /// Push `text` to the system clipboard and display a flash summarising
    /// what happened. Takes `&mut flash` rather than `&mut self` so it can be
    /// used from copy-mode branches that already borrow other parts of self.
    pub(super) fn yank_and_flash(
        flash: &mut Option<crate::ui::status_bar::FlashMessage>,
        text: &str,
    ) {
        match crate::actions_util::copy_to_clipboard(text) {
            Ok(()) => {
                let len = text.chars().count();
                *flash = Some(crate::ui::status_bar::FlashMessage::new(
                    format!("Copied {len} chars"),
                    std::time::Duration::from_secs(2),
                ));
            }
            Err(e) => {
                *flash = Some(crate::ui::status_bar::FlashMessage::new(
                    format!("Copy failed: {e}"),
                    std::time::Duration::from_secs(3),
                ));
            }
        }
    }
}

// ── Free helpers shared by the spawn_* methods ────────────────────────────────

/// Send `action` on the action channel, logging a warning if the receiver is
/// gone rather than silently dropping the message.
///
/// Every fetch task terminates by handing one action back to the event loop;
/// a dropped receiver means the TUI has already quit or the event thread
/// crashed. Neither case is recoverable here — but a warn-level log surfaces
/// receiver-shutdown races during development instead of leaving them
/// invisible under a `let _ =`.
fn send_or_warn(tx: &tokio::sync::mpsc::UnboundedSender<Action>, action: Action) {
    if let Err(err) = tx.send(action) {
        warn!("action channel closed; dropping fetch result: {err}");
    }
}

/// Spawn the supervisor + inner-task pair that performs a PR or issue detail
/// fetch and sends the resulting [`Action`] back on `tx`.
///
/// Shared by [`App::spawn_detail_fetch`] (foreground) and
/// [`App::spawn_detail_fetch_background`] (SWR / auto-refresh). The `label`
/// is used only in panic / abort error messages so foreground and background
/// failures are distinguishable in logs.
fn spawn_supervised_detail_fetch(
    client: std::sync::Arc<github::Client>,
    kind: DetailKind,
    repo: String,
    number: u32,
    tx: tokio::sync::mpsc::UnboundedSender<Action>,
    label: &'static str,
) {
    tokio::spawn(async move {
        let inner: tokio::task::JoinHandle<anyhow::Result<Action>> = tokio::spawn(async move {
            match kind {
                DetailKind::Pr => {
                    let detail = client.fetch_pr_detail(&repo, number).await?;
                    Ok(Action::PrDetailLoaded(Box::new(detail)))
                }
                DetailKind::Issue => {
                    let detail = client.fetch_issue_detail(&repo, number).await?;
                    Ok(Action::IssueDetailLoaded(Box::new(detail)))
                }
            }
        });
        let action = match inner.await {
            Ok(Ok(action)) => action,
            Ok(Err(e)) => Action::DetailFetchFailed(e.to_string()),
            Err(join_err) if join_err.is_panic() => {
                Action::DetailFetchFailed(format!("{label} task panicked: {join_err}"))
            }
            Err(join_err) => Action::DetailFetchFailed(format!("{label} task aborted: {join_err}")),
        };
        send_or_warn(&tx, action);
    });
}
