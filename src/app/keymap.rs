//! Key-event handlers for every focus state.

use crossterm::event::{KeyCode, KeyModifiers};

use crate::ui::pr_detail::DetailSection;

use super::actions::Action;
use super::state::App;
use super::types::{DetailKind, Focus, RepoPickerMode};

impl App {
    /// Translate a raw key event into an [`Action`] based on current focus.
    pub(super) fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        // Global bindings that work in any focus state. Repo-tab switching
        // takes priority even inside the detail view: switching from detail
        // pops back to the dashboard so the new tab's content is visible
        // (see `leave_detail_after_tab_switch`). The section-picker keys in
        // the detail view are the SHIFT-variants (`!@#$%`, `F`) precisely so
        // unshifted digits / Tab remain available here for tab switching.
        let typing_in_input =
            self.focus == Focus::RepoPicker && self.repo_picker_mode == RepoPickerMode::Input;
        if key.modifiers == KeyModifiers::NONE {
            match key.code {
                KeyCode::Char('q') => {
                    self.running = false;
                    return;
                }
                KeyCode::Char('?') => {
                    self.handle_action(Action::OpenHelp);
                    return;
                }
                KeyCode::Tab => {
                    self.handle_action(Action::NextTab);
                    return;
                }
                // `[` / `]` switch repo tabs everywhere EXCEPT Detail focus,
                // where they resize the sidebar (see `handle_key_detail`).
                KeyCode::Char(']') if !typing_in_input && self.focus != Focus::Detail => {
                    self.handle_action(Action::NextTab);
                    return;
                }
                KeyCode::Char('[') if !typing_in_input && self.focus != Focus::Detail => {
                    self.handle_action(Action::PrevTab);
                    return;
                }
                _ => {}
            }
        }

        if key.modifiers == KeyModifiers::SHIFT && key.code == KeyCode::BackTab {
            self.handle_action(Action::PrevTab);
            return;
        }

        // Digit keys 1–9 jump to the corresponding repo tab (1-based).
        // Suppressed only when the user is typing into the repo-picker Add
        // field. In detail focus the digits still dispatch SwitchTab — the
        // section-picker uses the SHIFT-variants (`!@#$%`) to avoid the
        // clash that bit us in Phase 1.
        if !typing_in_input
            && key.modifiers == KeyModifiers::NONE
            && let KeyCode::Char(ch) = key.code
            && let Some(digit) = ch.to_digit(10)
        {
            // digit==0 maps to tab index 9 (vim convention: 0 = 10th tab).
            let idx = if digit == 0 { 9 } else { (digit as usize) - 1 };
            self.handle_action(Action::SwitchTab(idx));
            return;
        }

        // Help overlay steals all other keys — Esc, '?', or 'q' closes it.
        if self.focus == Focus::Help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?' | 'q') => {
                    self.show_help = false;
                    self.focus = Focus::Dashboard;
                }
                _ => {}
            }
            return;
        }

        // Per-focus dispatch.
        match self.focus {
            Focus::Dashboard => self.handle_key_dashboard(key),
            Focus::FirstRun => self.handle_key_first_run(key),
            Focus::Detail => self.handle_key_detail(key),
            Focus::RepoPicker => self.handle_key_repo_picker(key),
            Focus::Confirm => self.handle_key_confirm(key),
            Focus::ThemePicker => self.handle_key_theme_picker(key),
            Focus::Help => {
                // Handled above; unreachable here.
            }
        }
    }

    /// Key handler for the first-run welcome wizard.
    ///
    /// Bindings:
    /// - `j` / `Down`: move cursor down (clamped).
    /// - `k` / `Up`: move cursor up (saturating).
    /// - `g g` (vim chord via `pending_g`): jump to top.
    /// - `G`: jump to bottom.
    /// - `Space`: toggle selected state of the cursor row.
    /// - `a`: open the repo-picker in Input mode so the user can add a custom repo.
    /// - `Enter`: commit all selected suggestions to config, then go to Dashboard.
    /// - `Esc`: skip wizard without committing, go to Dashboard.
    /// - `?`: toggle help overlay.
    /// - `q`: quit.
    pub(super) fn handle_key_first_run(&mut self, key: crossterm::event::KeyEvent) {
        // Ignore key combos with modifiers (consistent with other handlers).
        if key.modifiers != KeyModifiers::NONE {
            self.pending_g = false;
            return;
        }

        let len = self.first_run_suggestions.len();

        match key.code {
            KeyCode::Char('q') => {
                self.pending_g = false;
                self.running = false;
            }
            KeyCode::Char('?') => {
                self.pending_g = false;
                self.handle_action(Action::OpenHelp);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.pending_g = false;
                if len > 0 {
                    self.first_run_cursor = (self.first_run_cursor + 1).min(len - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.pending_g = false;
                self.first_run_cursor = self.first_run_cursor.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                if self.pending_g {
                    // Second `g` in vim chord — jump to top.
                    self.pending_g = false;
                    self.first_run_cursor = 0;
                } else {
                    self.pending_g = true;
                }
            }
            KeyCode::Char('G') => {
                self.pending_g = false;
                if len > 0 {
                    self.first_run_cursor = len - 1;
                }
            }
            KeyCode::Char(' ') => {
                self.pending_g = false;
                if len > 0 {
                    let idx = self.first_run_cursor.min(len - 1);
                    self.first_run_suggestions[idx].selected =
                        !self.first_run_suggestions[idx].selected;
                }
            }
            KeyCode::Char('a') => {
                // Open the repo-picker in Input mode so the user can add a
                // custom repo not present in the suggestions list.
                self.pending_g = false;
                self.repo_picker_list_cursor = 0;
                self.repo_picker_input.clear();
                self.repo_picker_mode = RepoPickerMode::Input;
                // Return to FirstRun (not Dashboard) when the picker closes,
                // so the wizard can stay open for any remaining suggestions.
                self.repo_picker_return_focus = Focus::FirstRun;
                self.focus = Focus::RepoPicker;
            }
            KeyCode::Enter => {
                self.pending_g = false;
                self.commit_first_run();
            }
            KeyCode::Esc => {
                // Skip without committing.
                self.pending_g = false;
                self.first_run_suggestions.clear();
                self.first_run_cursor = 0;
                self.focus = Focus::Dashboard;
                self.show_flash(
                    "Press `p` any time to add repos",
                    std::time::Duration::from_secs(2),
                );
            }
            _ => {
                self.pending_g = false;
            }
        }
    }

    /// Commit the user's selections from the first-run wizard to
    /// `config.repos`, save, and return to the dashboard.
    ///
    /// If nothing is selected, flashes a hint and keeps the wizard open
    /// rather than closing silently (users often hit Enter accidentally).
    pub(super) fn commit_first_run(&mut self) {
        // Collect the chosen slugs first so the mutation loop below does not
        // also need to borrow `first_run_suggestions`.
        let selected: Vec<String> = self
            .first_run_suggestions
            .iter()
            .filter(|s| s.selected)
            .map(|s| s.repo.clone())
            .collect();

        if selected.is_empty() {
            self.show_flash(
                "Nothing selected — press Space to pick repos, or Esc to skip",
                std::time::Duration::from_secs(3),
            );
            return;
        }

        // Explicit loop (not an iterator with side-effects in its closure) so
        // that the `added` count cannot silently desync from the `push` calls
        // if the deduplication logic is refactored later.
        let mut added: usize = 0;
        for repo in selected {
            if !self.config.repos.contains(&repo) {
                self.config.repos.push(repo);
                added += 1;
            }
        }
        self.config.save();
        self.sync_tabs_to_config();
        self.first_run_suggestions.clear();
        self.first_run_cursor = 0;
        self.focus = Focus::Dashboard;
        self.show_flash(
            format!("Added {added} repositor{}", if added == 1 { "y" } else { "ies" }),
            std::time::Duration::from_secs(2),
        );
    }

    /// Key handler for the PR/issue detail focus.
    #[allow(clippy::too_many_lines)]
    pub(super) fn handle_key_detail(&mut self, key: crossterm::event::KeyEvent) {
        // When copy mode is active, it owns the entire keymap for this focus.
        if self.copy_mode.active {
            self.handle_key_detail_copy_mode(key);
            return;
        }

        // Allow SHIFT so the section-picker keys (`F`, and `!@#$%` on US
        // keyboards / SHIFT+digit on those that send it unshifted) reach
        // the match arms. Reject Ctrl / Alt / Super as before.
        let blocked_mods = KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER;
        if key.modifiers.intersects(blocked_mods) {
            self.detail_pending_g = false;
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('b') => {
                self.detail_pending_g = false;
                self.back_to_dashboard();
            }
            KeyCode::Char('q') => {
                self.detail_pending_g = false;
                self.running = false;
            }
            KeyCode::Char('?') => {
                self.detail_pending_g = false;
                self.handle_action(Action::OpenHelp);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.detail_pending_g = false;
                let next = self.right_pane_scroll().saturating_add(1);
                *self.right_pane_scroll_mut() = next;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.detail_pending_g = false;
                let next = self.right_pane_scroll().saturating_sub(1);
                *self.right_pane_scroll_mut() = next;
            }
            KeyCode::Char('d') => {
                self.detail_pending_g = false;
                let next = self.right_pane_scroll().saturating_add(10);
                *self.right_pane_scroll_mut() = next;
            }
            KeyCode::Char('u') => {
                self.detail_pending_g = false;
                let next = self.right_pane_scroll().saturating_sub(10);
                *self.right_pane_scroll_mut() = next;
            }
            KeyCode::Char('g') => {
                if self.detail_pending_g {
                    self.detail_pending_g = false;
                    *self.right_pane_scroll_mut() = 0;
                } else {
                    self.detail_pending_g = true;
                }
            }
            KeyCode::Char('G') => {
                self.detail_pending_g = false;
                // Set to a large value; the renderer clamps to valid range.
                *self.right_pane_scroll_mut() = u16::MAX;
            }
            // J / K cycle the file cursor when the Files section is active.
            // Matches the "shift = bigger hop" vim convention: unshifted j/k
            // scroll within the current diff, shifted jumps to the next file.
            KeyCode::Char('J') if self.pr_detail_selected_section == DetailSection::Files => {
                self.detail_pending_g = false;
                self.cycle_files_cursor(1);
            }
            KeyCode::Char('K') if self.pr_detail_selected_section == DetailSection::Files => {
                self.detail_pending_g = false;
                self.cycle_files_cursor(-1);
            }
            // Section picker: `!@#$%` for Description/Checks/Reviews/Files/
            // Comments (SHIFT+1..5 on US keyboards). Terminals that deliver
            // SHIFT+digit without translating to punctuation are covered by
            // the second arm below. `F` is an additional shortcut to the
            // Files section that matches typical muscle memory.
            //
            // `n` / `N` are reserved for Phase 2 unresolved-thread cycling
            // and fall through to the wildcard no-op at the bottom.
            KeyCode::Char(ch @ ('!' | '@' | '#' | '$' | '%')) => {
                self.detail_pending_g = false;
                let idx = match ch {
                    '!' => 0,
                    '@' => 1,
                    '#' => 2,
                    '$' => 3,
                    '%' => 4,
                    _ => unreachable!(),
                };
                if let Some(&sec) = DetailSection::ALL.get(idx) {
                    self.pr_detail_selected_section = sec;
                    // `$` (index 3 = Files) enters overview mode.
                    if sec == DetailSection::Files {
                        self.pr_detail_files_show_diff = false;
                    }
                    self.copy_mode.h_scroll = 0;
                }
            }
            KeyCode::Char(ch @ '1'..='5') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.detail_pending_g = false;
                let idx = (ch as usize) - ('1' as usize);
                if let Some(&sec) = DetailSection::ALL.get(idx) {
                    self.pr_detail_selected_section = sec;
                    // SHIFT+4 enters Files overview mode.
                    if sec == DetailSection::Files {
                        self.pr_detail_files_show_diff = false;
                    }
                    self.copy_mode.h_scroll = 0;
                }
            }
            KeyCode::Char('F') => {
                // `F` jumps to Files in diff mode (drill-in gesture).
                self.detail_pending_g = false;
                self.pr_detail_selected_section = DetailSection::Files;
                self.pr_detail_files_show_diff = true;
                self.copy_mode.h_scroll = 0;
            }
            // Sidebar resize: `[` narrows (min 20), `]` widens (max 60).
            KeyCode::Char('[') if !self.sidebar_hidden => {
                self.detail_pending_g = false;
                self.sidebar_width = self.sidebar_width.saturating_sub(2).max(20);
            }
            KeyCode::Char(']') => {
                self.detail_pending_g = false;
                self.sidebar_width = self.sidebar_width.saturating_add(2).min(60);
            }
            // Toggle sidebar visibility.
            KeyCode::Char('\\') => {
                self.detail_pending_g = false;
                self.sidebar_hidden = !self.sidebar_hidden;
                let msg = if self.sidebar_hidden { "Sidebar hidden" } else { "Sidebar shown" };
                self.show_flash(msg, std::time::Duration::from_millis(1500));
            }
            KeyCode::Char('f') => {
                self.detail_pending_g = false;
                self.pr_detail_files_expanded = !self.pr_detail_files_expanded;
            }
            KeyCode::Char('m') => {
                self.detail_pending_g = false;
                self.detail_comments_expanded = !self.detail_comments_expanded;
            }
            KeyCode::Char('o') => {
                self.detail_pending_g = false;
                if let Some(url) = self.active_detail_url() {
                    let result = crate::actions_util::open_url_in_browser(&url);
                    self.flash_result(result, "Opened in browser", "Open failed");
                }
            }
            KeyCode::Char('y') => {
                self.detail_pending_g = false;
                if let Some(url) = self.active_detail_url() {
                    let result = crate::actions_util::copy_to_clipboard(&url);
                    self.flash_result(result, "URL copied", "Copy failed");
                }
            }
            KeyCode::Char('c') => {
                self.detail_pending_g = false;
                self.handle_action(Action::CheckoutBranch);
            }
            KeyCode::Char('v') => {
                self.detail_pending_g = false;
                // Enter copy mode at the top of the current viewport. Clamp
                // to the last real line so the cursor never strands on a
                // non-existent row.
                let lines = self.current_detail_lines();
                let last_row = lines.len().saturating_sub(1);
                let scroll = self.scroll_for(self.pr_detail_selected_section);
                let row = (scroll as usize).min(last_row);
                self.copy_mode.enter(row, 0);
            }
            KeyCode::Char('r') => {
                self.detail_pending_g = false;
                // Manual refresh bypasses the cache. Invalidate the entry so
                // the next restore is a cold miss (spinner) rather than a
                // stale-while-revalidate serve. `spawn_detail_fetch` owns the
                // `detail_fetching` guard — setting it externally would cause
                // the guard to skip the fetch.
                let target: Option<(DetailKind, String, u32)> = if let Some(detail) =
                    &self.pr_detail
                {
                    Some((DetailKind::Pr, detail.repo.clone(), detail.number))
                } else {
                    self.issue_detail
                        .as_ref()
                        .map(|detail| (DetailKind::Issue, detail.repo.clone(), detail.number))
                };

                if let Some((kind, repo, number)) = target {
                    match kind {
                        DetailKind::Pr => self.detail_cache.invalidate_pr(&repo, number),
                        DetailKind::Issue => self.detail_cache.invalidate_issue(&repo, number),
                    }
                    self.clear_detail_state();
                    if let Some(tx) = self.action_tx.clone() {
                        self.spawn_detail_fetch(kind, repo, number, tx);
                    }
                }
            }
            _ => {
                self.detail_pending_g = false;
            }
        }
    }

    /// Key handler active only while `self.copy_mode.active` is `true`.
    ///
    /// The copy-mode keymap is a separate modal layer: it owns `h`/`j`/`k`/`l`
    /// and arrows for cursor movement, `V` to toggle the selection anchor,
    /// `y` to yank the selection, and `Esc` to exit without copying.
    ///
    /// Any key not listed here is intentionally swallowed so the user cannot
    /// accidentally trigger destructive actions (like checkout) while trying
    /// to copy text.
    pub(super) fn handle_key_detail_copy_mode(&mut self, key: crossterm::event::KeyEvent) {
        let lines = self.current_detail_lines();
        match key.code {
            KeyCode::Esc => {
                self.copy_mode.exit();
            }
            KeyCode::Char('q') => {
                self.copy_mode.exit();
                self.running = false;
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.copy_mode.move_cursor(-1, 0, &lines);
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.copy_mode.move_cursor(1, 0, &lines);
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.copy_mode.move_cursor(0, 1, &lines);
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.copy_mode.move_cursor(0, -1, &lines);
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('g') => {
                self.copy_mode.jump_top();
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('G') => {
                self.copy_mode.jump_bottom(&lines);
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('V' | 'v') => {
                self.copy_mode.toggle_selection();
            }
            KeyCode::Char('0') | KeyCode::Home => {
                self.copy_mode.cursor.col = 0;
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('$') | KeyCode::End => {
                // Jump to the last character on the current row. Falls back to
                // 0 when the row is empty so the cursor never falls off the
                // end. Combined with `Y` below, this is the fastest path to
                // copy a long error message that overflows the viewport.
                let row = self.copy_mode.cursor.row;
                let last_col = lines
                    .get(row)
                    .map_or(0, |l| l.spans.iter().map(|s| s.content.chars().count()).sum::<usize>())
                    .saturating_sub(1);
                self.copy_mode.cursor.col = last_col;
                self.ensure_cursor_visible(&lines);
            }
            KeyCode::Char('Y') => {
                // One-shot "yank the current logical line to clipboard" —
                // huge QoL win for copying long error strings that wrap off
                // the right edge, where navigating `V` then `$` then `y` is
                // five keys for what should be one.
                let row = self.copy_mode.cursor.row;
                if let Some(line) = lines.get(row) {
                    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                    Self::yank_and_flash(&mut self.flash, &text);
                    self.copy_mode.exit();
                }
            }
            KeyCode::Char('y') => {
                if let Some(text) = self.copy_mode.selected_text(&lines) {
                    Self::yank_and_flash(&mut self.flash, &text);
                    self.copy_mode.exit();
                } else {
                    self.show_flash(
                        "No selection (press V to start, Y to yank whole line)",
                        std::time::Duration::from_secs(2),
                    );
                }
            }
            _ => {}
        }
    }

    /// Move the Files-sidebar cursor by `delta` rows, clamped to the PR's
    /// file list. Also nudges `pr_detail_sidebar_scroll` so the newly-
    /// selected file stays visible in the sidebar pane (best-effort —
    /// viewport height isn't cached for the sidebar, so we use a
    /// conservative default of 10 visible rows).
    pub(super) fn cycle_files_cursor(&mut self, delta: i32) {
        let Some(detail) = self.pr_detail.as_ref() else { return };
        if detail.files.is_empty() {
            return;
        }
        let last = detail.files.len().saturating_sub(1);
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let current = self.pr_detail_files_cursor as i32;
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let last_i = last as i32;
        let next = current.saturating_add(delta).clamp(0, last_i);
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        {
            self.pr_detail_files_cursor = next as usize;
        }
        // Nudge sidebar scroll so cursor stays roughly centred.
        #[allow(clippy::cast_possible_truncation)]
        let cursor_u16 = self.pr_detail_files_cursor as u16;
        let visible = 10u16;
        if cursor_u16 < self.pr_detail_sidebar_scroll {
            self.pr_detail_sidebar_scroll = cursor_u16;
        } else if cursor_u16 >= self.pr_detail_sidebar_scroll.saturating_add(visible) {
            self.pr_detail_sidebar_scroll = cursor_u16.saturating_sub(visible).saturating_add(1);
        }
    }

    // ── Repo picker ──────────────────────────────────────────────────────────

    /// Close the repo picker and sync tabs to the current config.
    ///
    /// Also resets picker state so the next `p` press starts fresh.
    pub(super) fn close_repo_picker(&mut self) {
        self.focus = self.repo_picker_return_focus;
        self.repo_picker_input.clear();
        self.repo_picker_mode = RepoPickerMode::List;
        self.sync_tabs_to_config();
    }

    /// Ensure `App::tabs` mirrors `config.repos`: open any newly-added repos,
    /// close any removed repos, and preserve the active tab where possible.
    ///
    /// Also drops stale entries from the `selection` map: a repo that has been
    /// removed from the config must not continue to hold a per-repo cursor
    /// index, or long-running sessions would accumulate dead entries.
    pub(super) fn sync_tabs_to_config(&mut self) {
        // Close tabs whose repos are no longer in the config and drop any
        // per-repo state that only makes sense for a tracked repo.
        let removed: Vec<(crate::ui::tabs::TabId, String)> = self
            .tabs
            .tabs
            .iter()
            .filter(|t| !self.config.repos.contains(&t.repo))
            .map(|t| (t.id, t.repo.clone()))
            .collect();
        for (id, repo) in removed {
            self.tabs.close(id);
            self.selection.remove(&repo);
        }

        // Open tabs for repos not yet represented.
        for repo in &self.config.repos {
            self.tabs.open_or_focus(repo);
        }

        // Restore cursor within bounds.
        let max_idx = self.config.repos.len().saturating_sub(1);
        if self.repo_picker_list_cursor > max_idx {
            self.repo_picker_list_cursor = max_idx;
        }
    }

    /// Key handler for the repo picker overlay.
    pub(super) fn handle_key_repo_picker(&mut self, key: crossterm::event::KeyEvent) {
        match self.repo_picker_mode {
            RepoPickerMode::List => self.handle_repo_picker_list_key(key),
            RepoPickerMode::Input => self.handle_repo_picker_input_key(key),
        }
    }

    /// Key handler for repo picker List mode.
    pub(super) fn handle_repo_picker_list_key(&mut self, key: crossterm::event::KeyEvent) {
        if key.modifiers != KeyModifiers::NONE {
            return;
        }
        let repo_count = self.config.repos.len();

        match key.code {
            KeyCode::Esc => {
                self.close_repo_picker();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if repo_count > 0 {
                    self.repo_picker_list_cursor =
                        (self.repo_picker_list_cursor + 1).min(repo_count - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.repo_picker_list_cursor = self.repo_picker_list_cursor.saturating_sub(1);
            }
            KeyCode::Char('d') | KeyCode::Backspace => {
                if !self.config.repos.is_empty() {
                    let idx = self.repo_picker_list_cursor.min(repo_count - 1);
                    self.config.repos.remove(idx);
                    self.config.save();
                    // Clamp cursor after removal.
                    let new_len = self.config.repos.len();
                    if new_len > 0 && self.repo_picker_list_cursor >= new_len {
                        self.repo_picker_list_cursor = new_len - 1;
                    } else if new_len == 0 {
                        self.repo_picker_list_cursor = 0;
                    }
                    self.sync_tabs_to_config();
                }
            }
            KeyCode::Char('a' | 'i') => {
                self.repo_picker_mode = RepoPickerMode::Input;
            }
            KeyCode::Enter => {
                // Focus the selected repo's tab and close the picker.
                if repo_count > 0 {
                    let idx = self.repo_picker_list_cursor.min(repo_count - 1);
                    let repo = self.config.repos[idx].clone();
                    self.tabs.open_or_focus(&repo);
                    self.close_repo_picker();
                }
            }
            _ => {}
        }
    }

    /// Key handler for repo picker Input mode.
    pub(super) fn handle_repo_picker_input_key(&mut self, key: crossterm::event::KeyEvent) {
        // Allow SHIFT (for uppercase letters in `0xIntuition`-style slugs).
        // Reject only Ctrl / Alt / Meta, which are not expected inside a
        // text field and would otherwise insert garbage characters on some
        // terminals.
        let blocked_mods = KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER;
        if key.modifiers.intersects(blocked_mods) {
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.repo_picker_mode = RepoPickerMode::List;
                self.repo_picker_input.clear();
            }
            KeyCode::Backspace => {
                self.repo_picker_input.pop();
            }
            KeyCode::Enter => {
                let slug = self.repo_picker_input.trim().to_owned();
                if !crate::ui::repo_picker::is_valid_repo_slug(&slug) {
                    self.show_flash(
                        format!("Invalid repo slug: \"{slug}\". Use owner/name format."),
                        std::time::Duration::from_secs(3),
                    );
                    return;
                }
                // Dedup: silently ignore if already tracked.
                if !self.config.repos.contains(&slug) {
                    self.config.repos.push(slug.clone());
                    self.config.save();
                    self.tabs.open_or_focus(&slug);
                    self.show_flash(format!("Added {slug}"), std::time::Duration::from_secs(2));
                }
                self.repo_picker_input.clear();
                // Stay in Input mode so the user can add more repos.
            }
            KeyCode::Char(c) => {
                self.repo_picker_input.push(c);
            }
            _ => {}
        }
    }

    /// Key handler for the confirmation overlay.
    pub(super) fn handle_key_confirm(&mut self, key: crossterm::event::KeyEvent) {
        if key.modifiers != KeyModifiers::NONE {
            return;
        }

        match key.code {
            KeyCode::Char('y') => {
                self.handle_action(Action::ConfirmCheckout(true));
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.handle_action(Action::ConfirmCheckout(false));
            }
            _ => {}
        }
    }

    /// Open the theme picker overlay, recording the current theme so `Esc` can
    /// revert it.
    ///
    /// Initialises `theme_picker_cursor` to the index of the currently active
    /// theme so the highlight starts on the user's existing choice.
    pub fn open_theme_picker(&mut self) {
        use crate::theme::Theme;

        // Find the index of the current theme in the ordered list; default 0.
        let current_idx = Theme::ALL.iter().position(|&t| t == self.config.theme).unwrap_or(0);

        self.theme_picker_original = self.config.theme;
        self.theme_picker_cursor = current_idx;
        self.theme_picker_return_focus = self.focus;
        self.focus = Focus::ThemePicker;
    }

    /// Key handler for the theme picker overlay ([`Focus::ThemePicker`]).
    ///
    /// Bindings:
    /// - `j` / `Down`: move cursor down (wraps around).
    /// - `k` / `Up`: move cursor up (wraps around).
    /// - `Enter`: apply the highlighted theme, persist config, close.
    /// - `Esc`: restore the original theme (in-memory, no save), close.
    pub(super) fn handle_key_theme_picker(&mut self, key: crossterm::event::KeyEvent) {
        use crate::theme::{Palette, Theme};

        if key.modifiers != KeyModifiers::NONE {
            return;
        }

        let count = Theme::ALL.len();

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                // Wrap: last item → first item.
                self.theme_picker_cursor = (self.theme_picker_cursor + 1) % count;
                // Live preview.
                self.palette = Palette::from_theme(Theme::ALL[self.theme_picker_cursor]);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                // Wrap: first item → last item.
                self.theme_picker_cursor = (self.theme_picker_cursor + count - 1) % count;
                // Live preview.
                self.palette = Palette::from_theme(Theme::ALL[self.theme_picker_cursor]);
            }
            KeyCode::Enter => {
                // Apply and persist.
                let chosen = Theme::ALL[self.theme_picker_cursor];
                self.config.theme = chosen;
                self.palette = Palette::from_theme(chosen);
                self.config.save();
                self.focus = self.theme_picker_return_focus;
                self.show_flash(
                    format!("Theme applied: {}", chosen.label()),
                    std::time::Duration::from_secs(3),
                );
            }
            KeyCode::Esc => {
                // Revert to the original theme (in-memory only; no save).
                let original = self.theme_picker_original;
                self.palette = Palette::from_theme(original);
                // config.theme was never written, so it still holds the original
                // value — no assignment needed here.
                self.focus = self.theme_picker_return_focus;
                self.show_flash("Theme change cancelled", std::time::Duration::from_secs(2));
            }
            _ => {}
        }
    }

    /// Key handler for the dashboard (PR/issue list) focus.
    #[allow(clippy::too_many_lines)] // dispatcher: each arm is one key binding
    pub(super) fn handle_key_dashboard(&mut self, key: crossterm::event::KeyEvent) {
        // Allow SHIFT (used for `A`, `G`, `R`, `N`) but reject Ctrl/Alt/Super
        // so a stray `Ctrl+c` or `Alt+j` can't silently trigger list moves.
        let blocked_mods = KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER;
        if key.modifiers.intersects(blocked_mods) {
            self.pending_g = false;
            return;
        }

        // Resolve active repo slug once.
        let active_repo = self.tabs.active_tab().map(|t| t.repo.clone());

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.pending_g = false;
                if let Some(repo) = active_repo {
                    let len = self.active_list_len();
                    if len > 0 {
                        let sel = self.selection.entry(repo).or_insert(0);
                        *sel = (*sel + 1).min(len - 1);
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.pending_g = false;
                if let Some(repo) = active_repo {
                    let sel = self.selection.entry(repo).or_insert(0);
                    *sel = sel.saturating_sub(1);
                }
            }
            KeyCode::Char('g') => {
                if self.pending_g {
                    // Second `g` — jump to top.
                    self.pending_g = false;
                    if let Some(repo) = active_repo {
                        self.selection.insert(repo, 0);
                    }
                } else {
                    self.pending_g = true;
                }
            }
            KeyCode::Char('G') => {
                self.pending_g = false;
                if let Some(repo) = active_repo {
                    let len = self.active_list_len();
                    let bottom = if len > 0 { len - 1 } else { 0 };
                    self.selection.insert(repo, bottom);
                }
            }
            KeyCode::Char('i') => {
                self.pending_g = false;
                self.handle_action(Action::ToggleView);
            }
            KeyCode::Char('r') => {
                self.pending_g = false;
                self.handle_action(Action::Refresh);
            }
            KeyCode::Char('R') => {
                self.pending_g = false;
                self.handle_action(Action::RefreshAll);
            }
            KeyCode::Enter => {
                self.pending_g = false;
                self.open_detail_for_selection();
            }
            KeyCode::Char('o') => {
                self.pending_g = false;
                if let Some(url) = self.dashboard_selected_url() {
                    let result = crate::actions_util::open_url_in_browser(&url);
                    self.flash_result(result, "Opened in browser", "Open failed");
                }
            }
            KeyCode::Char('y') => {
                self.pending_g = false;
                if let Some(url) = self.dashboard_selected_url() {
                    let result = crate::actions_util::copy_to_clipboard(&url);
                    self.flash_result(result, "URL copied", "Copy failed");
                }
            }
            KeyCode::Char('c') => {
                self.pending_g = false;
                // Open the theme picker. The checkout action is available in
                // the detail view where `c` retains its original binding.
                self.open_theme_picker();
            }
            KeyCode::Char('p') => {
                tracing::debug!("dashboard: 'p' pressed — dispatching OpenRepoPicker");
                self.pending_g = false;
                self.handle_action(Action::OpenRepoPicker);
            }
            KeyCode::Char('A') => {
                self.pending_g = false;
                self.handle_action(Action::ToggleShowAll);
            }
            KeyCode::Char('f') => {
                self.pending_g = false;
                tracing::info!("Phase 4: not yet implemented — filter");
            }
            KeyCode::Char('n') => {
                self.pending_g = false;
                tracing::info!("Phase 4: not yet implemented — next match");
            }
            KeyCode::Char('N') => {
                self.pending_g = false;
                tracing::info!("Phase 4: not yet implemented — prev match");
            }
            KeyCode::Char('b') => {
                self.pending_g = false;
                tracing::info!("Phase 4: not yet implemented — back");
            }
            // All other keys (including Esc) cancel any pending chord.
            _ => {
                self.pending_g = false;
            }
        }
    }

    // ── Detail-view helpers ────────────────────────────────────────────────────

    /// Return the URL of whichever detail object is currently loaded (PR first,
    /// issue as fallback). `None` when neither is populated — which can happen
    /// if the user hits a URL-consuming key before the first detail fetch
    /// lands.
    fn active_detail_url(&self) -> Option<String> {
        self.pr_detail
            .as_ref()
            .map(|d| d.url.clone())
            .or_else(|| self.issue_detail.as_ref().map(|d| d.url.clone()))
    }

    /// Return the URL of the PR or issue currently highlighted on the
    /// dashboard. `None` when no inbox is loaded, no tab is active, or the
    /// selection index is out of range.
    fn dashboard_selected_url(&self) -> Option<String> {
        let repo = self.tabs.active_tab()?.repo.clone();
        let inbox = self.inbox.as_ref()?;
        let sel = self.selection.get(&repo).copied().unwrap_or(0);
        match self.session.view_mode(&repo) {
            crate::state::ViewMode::Prs => inbox
                .prs
                .iter()
                .filter(|pr| pr.repo == repo)
                .nth(sel)
                .map(|pr| pr.url.clone()),
            crate::state::ViewMode::Issues => inbox
                .issues
                .iter()
                .filter(|i| i.repo == repo)
                .nth(sel)
                .map(|i| i.url.clone()),
        }
    }

    /// Show a status-bar flash for a `Result<()>` OS action. The success
    /// message gets a 2s duration; the error message gets 3s so users have
    /// time to read the wrapped error detail.
    fn flash_result(&mut self, result: anyhow::Result<()>, ok_msg: &str, err_prefix: &str) {
        match result {
            Ok(()) => self.show_flash(ok_msg.to_owned(), std::time::Duration::from_secs(2)),
            Err(e) => self.show_flash(
                format!("{err_prefix}: {e}"),
                std::time::Duration::from_secs(3),
            ),
        }
    }

    /// Reset every per-detail scroll / payload / refresh-flag field. Shared
    /// by the `'r'` manual-refresh path so a future detail kind doesn't
    /// silently skip part of the reset.
    fn clear_detail_state(&mut self) {
        self.detail_refreshing = None;
        self.pr_detail = None;
        self.issue_detail = None;
        self.detail_error = None;
        self.pr_detail_scroll.clear();
        self.pr_detail_diff_scroll.clear();
    }
}
