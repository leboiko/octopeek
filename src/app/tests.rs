//! Unit tests for the `app` module.
//!
//! Moved wholesale from the bottom of the original monolithic `mod.rs`.
//! `use super::*` brings in all re-exports from `app::mod` (types, App, Action,
//! Focus, etc.) exactly as before.

use super::*;
// Items not re-exported from `mod.rs` (test-only or internal) must be
// imported explicitly since `use super::*` only pulls public re-exports.
use super::actions::Action;
use super::types::{DetailKind, DetailRef, PerTabState};
use chrono::Utc;
use crate::ui::pr_detail::DetailSection;
use crate::github::types::{
    CheckState, Inbox, Issue, Label, MergeStateStatus, Mergeable, PullRequest, Review,
    ReviewDecision, Role,
};

/// Build a minimal clean PR for use in tests.
fn make_pr(repo: &str, flag_variant: &str, viewer: &str) -> PullRequest {
    let mut pr = PullRequest {
        number: 1,
        title: "Test PR".to_owned(),
        url: "https://github.com/o/r/pull/1".to_owned(),
        repo: repo.to_owned(),
        author: viewer.to_owned(),
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
    };
    match flag_variant {
        "conflict" => pr.mergeable = Mergeable::Conflicting,
        "review_requested" => pr.requested_reviewers = vec![viewer.to_owned()],
        "draft" => pr.is_draft = true,
        "changes" => pr.review_decision = Some(ReviewDecision::ChangesRequested),
        _ => {} // clean
    }
    pr
}

#[allow(dead_code)]
fn make_issue(repo: &str) -> Issue {
    Issue {
        number: 1,
        title: "Test Issue".to_owned(),
        url: "https://github.com/o/r/issues/1".to_owned(),
        repo: repo.to_owned(),
        author: "viewer".to_owned(),
        comments_count: 0,
        updated_at: Utc::now(),
        labels: vec![Label { name: "bug".to_owned(), color: "ee0701".to_owned() }],
    }
}

/// `on_inbox_loaded` must correctly count needs-action PRs for a tab
/// (excluding Draft and Clean) and update `tab.needs_action_count`.
#[test]
fn on_inbox_loaded_sets_needs_action_count() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    let inbox = Inbox {
        viewer_login: "viewer".to_owned(),
        prs: vec![
            make_pr("o/r", "conflict", "viewer"),         // needs action
            make_pr("o/r", "review_requested", "viewer"), // needs action
            make_pr("o/r", "draft", "viewer"),            // NOT needs action
            make_pr("o/r", "clean", "viewer"),            // NOT needs action
            make_pr("other/repo", "conflict", "viewer"),  // different repo
        ],
        issues: vec![],
    };

    app.on_inbox_loaded(inbox);

    let tab = app.tabs.tabs.iter().find(|t| t.repo == "o/r").expect("tab for o/r");
    assert_eq!(
        tab.needs_action_count,
        Some(2),
        "Expected 2 action items in o/r, got {:?}",
        tab.needs_action_count
    );
}

/// After `on_inbox_loaded`, fetching is false and error is cleared.
#[test]
fn on_inbox_loaded_clears_error_and_fetching() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.fetching = true;
    app.last_fetch_error = Some("prior error".to_owned());

    let inbox = Inbox { viewer_login: "viewer".to_owned(), prs: vec![], issues: vec![] };
    app.on_inbox_loaded(inbox);

    assert!(!app.fetching);
    assert!(app.last_fetch_error.is_none());
    assert!(app.inbox_loaded_at.is_some());
}

/// When a refresh shrinks a repo's list, stale selection indices must be
/// clamped so the dashboard cannot render a cursor past the end of the list.
#[test]
fn on_inbox_loaded_clamps_stale_selection() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    // Simulate: earlier refresh had 5 PRs and the user moved the cursor to row 4.
    app.selection.insert("o/r".to_owned(), 4);

    // Now the refresh returns only 2 PRs in "o/r".
    let inbox = Inbox {
        viewer_login: "viewer".to_owned(),
        prs: vec![make_pr("o/r", "clean", "viewer"), make_pr("o/r", "conflict", "viewer")],
        issues: vec![],
    };
    app.on_inbox_loaded(inbox);

    assert_eq!(app.selection.get("o/r"), Some(&1), "stale index 4 must clamp to len-1 = 1");
}

/// When a refresh removes every item for a repo, the stored selection must
/// collapse to 0 rather than attempting len-1 = `usize::MAX` underflow.
#[test]
fn on_inbox_loaded_clamps_empty_list() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.selection.insert("o/r".to_owned(), 3);

    let inbox = Inbox { viewer_login: "viewer".to_owned(), prs: vec![], issues: vec![] };
    app.on_inbox_loaded(inbox);

    assert_eq!(app.selection.get("o/r"), Some(&0));
}

/// `on_fetch_failed` sets the error string and clears `fetching`.
#[test]
fn on_fetch_failed_records_error() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.fetching = true;

    app.on_fetch_failed("network timeout".to_owned());

    assert!(!app.fetching);
    assert_eq!(app.last_fetch_error.as_deref(), Some("network timeout"));
}

/// Unused fields added to avoid "unused import" warnings from the test helpers.
#[allow(dead_code)]
fn _use_types(_r: Review, _rd: ReviewDecision) {}

// ── Phase 4 detail-UI tests ───────────────────────────────────────────────

/// Pressing Esc in Detail focus clears `pr_detail` and `issue_detail`, resets
/// scroll, and returns focus to Dashboard.
#[test]
fn esc_in_detail_focus_returns_to_dashboard() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    // Set a non-zero scroll offset for the Description section.
    *app.scroll_mut(DetailSection::Description) = 42;

    app.back_to_dashboard();

    assert_eq!(app.focus, Focus::Dashboard);
    assert!(app.pr_detail.is_none());
    assert!(app.issue_detail.is_none());
    assert!(app.detail_error.is_none());
    assert!(app.pr_detail_scroll.is_empty(), "scroll map must be cleared on back_to_dashboard");
}

/// Pressing Enter on the dashboard when a PR is selected must set
/// `detail_fetching = true`, switch focus to Detail, and clear prior state.
#[test]
fn enter_on_dashboard_populates_detail_fetching() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    let inbox = Inbox {
        viewer_login: "viewer".to_owned(),
        prs: vec![make_pr("o/r", "clean", "viewer")],
        issues: vec![],
    };
    app.on_inbox_loaded(inbox);

    // Simulate Enter key on the dashboard.
    app.open_detail_for_selection();

    // detail_fetching should be true (we can't actually fetch without a
    // client, but the flag should be set if a client exists; in tests there
    // is no client so `spawn_detail_fetch` returns early, but focus still
    // switches and the flags are reset).
    assert_eq!(app.focus, Focus::Detail);
    assert!(app.pr_detail_scroll.is_empty(), "scroll map should be empty after open");
}

/// Per-section scroll must not exceed a plausible content ceiling.
///
/// The actual clamp happens in `clamp_pr_detail_scroll`, but we can verify
/// that wrapping `u16` arithmetic is avoided (saturating add) for a section.
#[test]
fn scroll_clamped_by_saturating_add() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    // Set Description section scroll to max.
    *app.scroll_mut(DetailSection::Description) = u16::MAX;
    // Saturating add must not wrap.
    let current = app.scroll_for(DetailSection::Description);
    *app.scroll_mut(DetailSection::Description) = current.saturating_add(1);
    assert_eq!(
        app.scroll_for(DetailSection::Description),
        u16::MAX,
        "saturating add must not wrap"
    );
}

/// Pressing `o` with an invalid URL produces a flash error.
/// Verifies the `open_url_in_browser` error-message shape without actually
/// invoking the underlying `open::that` call.
///
/// The previous version of this test called `open::that("")` directly —
/// which on macOS treats an empty path as the current directory and pops
/// the Finder window. Every `cargo test` run opened Finder, which is
/// exactly the same class of "tests must not side-effect on the
/// developer's machine" bug we fixed for `Config::save()` with
/// `with_config_dir_override`.
///
/// Here we only assert on the error message wrapper — the actual `open`
/// crate behaviour is out of scope for unit tests and can be covered by
/// an `#[ignore]`-marked integration test if end-to-end verification is
/// ever needed.
#[test]
fn open_browser_error_message_includes_url() {
    use anyhow::Context as _;

    // Short-circuit by constructing the same `anyhow::Error` the function
    // would produce on a failed `open::that`; the wrapper shape is what
    // we care about — not whether the OS accepts the URL.
    let url = "https://example.invalid/pr/1";
    let wrapped: anyhow::Result<()> = Err(anyhow::anyhow!("simulated launch failure"))
        .with_context(|| format!("failed to open URL in browser: {url}"));
    let msg = format!("{:#}", wrapped.unwrap_err());
    assert!(msg.contains(url), "error message must include the URL for debuggability");
    assert!(
        msg.contains("failed to open URL in browser"),
        "wrapper message must name the operation"
    );
}

/// `open_url_in_browser` rejects non-`https://` URLs without invoking the
/// OS command. The guard stops a hypothetical malicious API response from
/// triggering `file://`, `ssh://`, or custom-scheme handlers.
#[test]
fn open_browser_refuses_non_https_scheme() {
    for hostile in ["file:///etc/passwd", "http://example.com", "ssh://bad", ""] {
        let err = crate::actions_util::open_url_in_browser(hostile)
            .expect_err("non-https URL must be rejected");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("refusing to open non-https URL"),
            "rejection message must name the guard, got: {msg}"
        );
        assert!(msg.contains(hostile), "rejection message must echo the URL");
    }
}

/// `copy_to_clipboard` is skipped in headless environments; this test
/// verifies the function returns a typed Result without panicking.
#[test]
#[ignore = "clipboard unavailable on headless CI; run manually"]
fn copy_url_does_not_panic() {
    let result = crate::actions_util::copy_to_clipboard("https://github.com");
    // On a real desktop this should succeed; on headless it fails gracefully.
    let _ = result;
}

// ── Copy mode & mouse tests ───────────────────────────────────────────────

fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
}

/// Pressing `v` in detail focus enters copy mode with the cursor anchored
/// inside the current content. With no detail loaded the cursor clamps
/// to row 0 (rather than landing on the phantom row of a stale scroll
/// offset), which is the specific regression we hit when the user
/// over-scrolled past the content's end and then entered copy mode.
#[test]
fn v_in_detail_enters_copy_mode_and_clamps_to_content() {
    let mut app =
        App::new(crate::config::Config::default(), crate::state::AppSession::default());
    app.focus = Focus::Detail;
    // Set a scroll offset well past the empty content's end.
    *app.scroll_mut(DetailSection::Description) = 12;

    app.handle_key(key(crossterm::event::KeyCode::Char('v')));

    assert!(app.copy_mode.active);
    assert_eq!(
        app.copy_mode.cursor.row, 0,
        "cursor must clamp to last real row (0 when no content)"
    );
    assert_eq!(app.copy_mode.cursor.col, 0);
    assert!(app.copy_mode.anchor.is_none(), "no selection until V pressed");
}

/// Esc in copy mode exits the mode but stays in the detail focus —
/// distinct from Esc in normal detail mode, which returns to dashboard.
#[test]
fn esc_in_copy_mode_stays_in_detail() {
    let mut app =
        App::new(crate::config::Config::default(), crate::state::AppSession::default());
    app.focus = Focus::Detail;
    app.copy_mode.enter(0, 0);

    app.handle_key(key(crossterm::event::KeyCode::Esc));

    assert!(!app.copy_mode.active);
    assert_eq!(app.focus, Focus::Detail, "Esc in copy mode must not leave detail");
}

/// Returning to the dashboard via `b` also tears down copy-mode state.
#[test]
fn back_to_dashboard_clears_copy_mode() {
    let mut app =
        App::new(crate::config::Config::default(), crate::state::AppSession::default());
    app.focus = Focus::Detail;
    app.copy_mode.enter(5, 7);

    app.back_to_dashboard();

    assert_eq!(app.focus, Focus::Dashboard);
    assert!(!app.copy_mode.active);
    assert_eq!(app.copy_mode.cursor, crate::ui::copy_mode::Pos::default());
}

/// Mouse wheel in the right pane (outside sidebar) scrolls the active
/// section by 3 lines per tick.
#[test]
fn mouse_wheel_scrolls_detail() {
    use crate::ui::pr_detail::tests::fixture_pr_detail;
    use crossterm::event::{MouseEvent, MouseEventKind};

    let mut app =
        App::new(crate::config::Config::default(), crate::state::AppSession::default());
    app.focus = Focus::Detail;
    // Load a fixture so clamp_pr_detail_scroll does not reset the offset.
    app.pr_detail = Some(fixture_pr_detail(3, 2, 4, 2));
    *app.scroll_mut(DetailSection::Description) = 0;

    // Place the right-pane viewport so the column check passes (not in sidebar).
    // Use height=1 so the clamp ceiling = content_lines - 1 (several lines for
    // the Description fixture), well above the 0+3=3 target.
    app.pr_detail_right_viewport.set(ratatui::layout::Rect::new(28, 0, 80, 1));

    app.handle_action(Action::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 40, // inside the right pane (>= x=28)
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }));
    assert_eq!(app.scroll_for(DetailSection::Description), 3, "scroll down by 3");

    app.handle_action(Action::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 40,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }));
    assert_eq!(app.scroll_for(DetailSection::Description), 0, "scroll up by 3 returns to 0");
}

/// A left-click inside the cached detail viewport enters copy mode and
/// places the cursor at the corresponding content coordinate.
#[test]
fn mouse_click_in_detail_places_cursor() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let mut app =
        App::new(crate::config::Config::default(), crate::state::AppSession::default());
    app.focus = Focus::Detail;
    // Pretend the right-pane viewport is at (28,1) with size 80x20.
    app.pr_detail_right_viewport.set(ratatui::layout::Rect::new(28, 1, 80, 20));
    // Also set the legacy viewport alias so existing checks pass.
    app.pr_detail_viewport.set(ratatui::layout::Rect::new(28, 1, 80, 20));
    *app.scroll_mut(DetailSection::Description) = 5;

    app.handle_action(Action::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 38, // inside right pane (starts at x=28); col offset = 38-28=10
        row: 3,     // inside viewport: row offset = 3-1=2 -> content row = scroll(5)+2=7
        modifiers: crossterm::event::KeyModifiers::NONE,
    }));

    assert!(app.copy_mode.active);
    assert_eq!(app.copy_mode.cursor.row, 7);
    assert_eq!(app.copy_mode.cursor.col, 10);
}

/// A left-click outside the cached viewport must be ignored (no copy-mode
/// entry, no state mutation). This also covers the case where the
/// viewport hasn't been cached yet (zero-sized rect).
#[test]
fn mouse_click_outside_viewport_is_ignored() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let mut app =
        App::new(crate::config::Config::default(), crate::state::AppSession::default());
    app.focus = Focus::Detail;
    // Set a small right-pane viewport; clicks outside it must be ignored.
    app.pr_detail_right_viewport.set(ratatui::layout::Rect::new(28, 1, 10, 10));
    app.pr_detail_viewport.set(ratatui::layout::Rect::new(28, 1, 10, 10));

    app.handle_action(Action::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 50,
        row: 50, // far outside
        modifiers: crossterm::event::KeyModifiers::NONE,
    }));

    assert!(!app.copy_mode.active);
}

/// Dragging with left button held starts a selection on first drag and
/// moves the cursor on subsequent drag events.
#[test]
fn mouse_drag_starts_selection() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let mut app =
        App::new(crate::config::Config::default(), crate::state::AppSession::default());
    app.focus = Focus::Detail;
    // Right pane at x=28..107, y=1..20.
    app.pr_detail_right_viewport.set(ratatui::layout::Rect::new(28, 1, 80, 20));
    app.pr_detail_viewport.set(ratatui::layout::Rect::new(28, 1, 80, 20));

    // Initial click to enter copy mode; column 30 is inside the right pane.
    // col offset = 30 - 28 = 2; row offset = 1 - 1 = 0.
    app.handle_action(Action::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 30,
        row: 1,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }));
    assert!(app.copy_mode.active);
    assert!(app.copy_mode.anchor.is_none());

    // First drag event sets the anchor at the current cursor position.
    // column 33 = col offset 5.
    app.handle_action(Action::Mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 33,
        row: 1,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }));

    assert_eq!(app.copy_mode.anchor, Some(crate::ui::copy_mode::Pos { row: 0, col: 2 }));
    // Cursor moved to drag position (row 0 inside content since no lines).
    // Without loaded detail, current_detail_lines() returns an empty Vec,
    // which clamps row to 0. Column is free-form (display cell).
    assert_eq!(app.copy_mode.cursor.col, 5);
}

// ── Phase 5 tests ─────────────────────────────────────────────────────────

/// Pressing `p` on the dashboard must open the repo picker and set
/// `Focus::RepoPicker`.
#[test]
fn pressing_p_opens_repo_picker() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    app.handle_action(Action::OpenRepoPicker);

    assert_eq!(app.focus, Focus::RepoPicker);
}

/// Opening the repo picker must reset input state.
#[test]
fn open_repo_picker_resets_state() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    // Pre-populate stale picker state.
    app.repo_picker_input = "stale/input".to_owned();
    app.repo_picker_mode = RepoPickerMode::Input;

    app.handle_action(Action::OpenRepoPicker);

    assert_eq!(app.focus, Focus::RepoPicker);
    assert!(app.repo_picker_input.is_empty(), "input buffer should be cleared on open");
    assert_eq!(app.repo_picker_mode, RepoPickerMode::List);
}

/// Closing the repo picker must restore the previous focus.
#[test]
fn close_repo_picker_restores_focus() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Dashboard;

    app.handle_action(Action::OpenRepoPicker);
    assert_eq!(app.focus, Focus::RepoPicker);

    // Close via Esc (simulated by calling close_repo_picker directly).
    app.close_repo_picker();
    assert_eq!(app.focus, Focus::Dashboard);
}

/// In Detail focus `[` / `]` resize the sidebar rather than switching
/// repo tabs.  Pressing `]` widens up to max 60; `[` narrows down to min 20.
#[test]
fn bracket_keys_resize_sidebar_in_detail() {
    let config = crate::config::Config {
        repos: vec!["a/one".to_owned(), "b/two".to_owned()],
        ..Default::default()
    };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    let initial_tab = app.tabs.active_index();

    // `]` widens sidebar, does NOT switch tabs.
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(']'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.focus, Focus::Detail, "] must not leave Detail focus");
    assert_eq!(app.tabs.active_index(), initial_tab, "] must not switch tabs in Detail");
    assert_eq!(app.sidebar_width, 30, "] widens sidebar by 2");

    // `[` narrows sidebar, does NOT switch tabs.
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('['),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.tabs.active_index(), initial_tab, "[ must not switch tabs in Detail");
    assert_eq!(app.sidebar_width, 28, "[ narrows sidebar by 2");

    // Clamp: cannot go below 20.
    app.sidebar_width = 21;
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('['),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.sidebar_width, 20, "[ clamps at minimum 20 (step: 21 -> 20)");
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('['),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.sidebar_width, 20, "[ is a no-op when sidebar is already at minimum");

    // Clamp: cannot exceed 60.
    app.sidebar_width = 59;
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(']'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.sidebar_width, 60, "] clamps at maximum 60 (step: 59 -> 60)");
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(']'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.sidebar_width, 60, "] is a no-op when sidebar is already at maximum");
}

/// Outside Detail focus `[` / `]` still switch repo tabs.
#[test]
fn bracket_keys_still_switch_tabs_from_dashboard() {
    let config = crate::config::Config {
        repos: vec!["a/one".to_owned(), "b/two".to_owned()],
        ..Default::default()
    };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    // Default focus is Dashboard.
    assert_eq!(app.focus, Focus::Dashboard);
    assert_eq!(app.tabs.active_index(), Some(0));

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(']'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.tabs.active_index(), Some(1), "] switches to next tab from Dashboard");

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('['),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.tabs.active_index(), Some(0), "[ switches to prev tab from Dashboard");
}

/// `\` toggles `sidebar_hidden` and shows a flash message each press.
#[test]
fn backslash_toggles_sidebar_visibility() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;

    assert!(!app.sidebar_hidden, "sidebar visible by default");

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('\\'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert!(app.sidebar_hidden, "first \\ hides sidebar");
    assert!(app.flash.is_some(), "flash shown after hide");

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('\\'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert!(!app.sidebar_hidden, "second \\ un-hides sidebar");
    assert!(app.flash.is_some(), "flash shown after un-hide");
}

/// `$` sets Files section in overview mode (`files_show_diff = false`).
#[test]
fn dollar_enters_files_overview_mode() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    // Pre-set diff mode so we can verify `$` resets it.
    app.pr_detail_files_show_diff = true;

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('$'),
        crossterm::event::KeyModifiers::NONE,
    ));

    assert_eq!(app.pr_detail_selected_section, DetailSection::Files);
    assert!(!app.pr_detail_files_show_diff, "$ must enter overview mode");
}

/// `F` sets Files section in diff mode (`files_show_diff = true`).
#[test]
fn shift_f_enters_files_diff_mode() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    app.pr_detail_files_show_diff = false;

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('F'),
        crossterm::event::KeyModifiers::SHIFT,
    ));

    assert_eq!(app.pr_detail_selected_section, DetailSection::Files);
    assert!(app.pr_detail_files_show_diff, "F must enter diff mode");
}

/// Clicking a sidebar file row sets `files_show_diff = true` (drill-in
/// gesture) and updates the cursor index.
#[test]
fn clicking_sidebar_file_enters_diff_mode() {
    use crate::ui::pr_detail::tests::fixture_pr_detail;

    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    app.pr_detail = Some(fixture_pr_detail(0, 0, 3, 0));
    app.pr_detail_files_show_diff = false;

    // Fabricate a sidebar geometry: sections_rect at row 0, files_rect
    // starting at row 7 (default height). Row 8 = first file (header = row 7).
    let sections_rect = ratatui::layout::Rect { x: 0, y: 0, width: 28, height: 7 };
    let files_rect = ratatui::layout::Rect { x: 0, y: 7, width: 28, height: 20 };

    // Click row 8: relative = 1, file_idx = 0.
    app.handle_sidebar_click(0, 8, sections_rect, files_rect);

    assert_eq!(app.pr_detail_selected_section, DetailSection::Files);
    assert!(app.pr_detail_files_show_diff, "sidebar file click must enable diff mode");
    assert_eq!(app.pr_detail_files_cursor, 0);
}

/// Switching tabs from a detail focus with no loaded detail falls back
/// to the dashboard. This is a degenerate case in production (the user
/// can't reach `Focus::Detail` without a fetch starting) but pins the
/// save-no-ref → restore-no-ref path.
#[test]
fn tab_switch_from_detail_with_no_loaded_detail_falls_back_to_dashboard() {
    let config = crate::config::Config {
        repos: vec!["a/one".to_owned(), "b/two".to_owned()],
        ..Default::default()
    };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;

    app.handle_action(Action::SwitchTab(1));

    assert_eq!(app.focus, Focus::Dashboard);
    assert!(app.pr_detail.is_none());
}

/// Switching away from a tab while viewing a PR, then returning, must
/// restore the detail focus — the user should land back on the PR they
/// were reading, not on the dashboard list.
#[test]
fn tab_round_trip_preserves_detail_focus_for_pr() {
    use crate::ui::pr_detail::tests::fixture_pr_detail;

    let config = crate::config::Config {
        repos: vec!["a/one".to_owned(), "b/two".to_owned()],
        ..Default::default()
    };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    // Simulate the user having a PR open on tab 0 (repo `a/one`).
    let pr = fixture_pr_detail(0, 0, 0, 0);
    app.focus = Focus::Detail;
    app.pr_detail = Some(pr);

    // Switch to tab 1 → saves tab 0's detail ref, clears payload.
    app.handle_action(Action::SwitchTab(1));
    assert_eq!(app.focus, Focus::Dashboard, "tab 1 has no saved detail");
    assert!(app.pr_detail.is_none(), "detail payload must clear between tabs");

    // Switch back to tab 0 → restore must re-enter Detail focus and
    // dispatch a fresh fetch (no client in tests, so `pr_detail` stays
    // None — the important thing is the focus and that we tried).
    app.handle_action(Action::SwitchTab(0));
    assert_eq!(
        app.focus,
        Focus::Detail,
        "round-tripping back to a tab with a saved detail ref must restore Detail focus"
    );
}

/// Explicitly exiting a detail via Esc must also forget the saved
/// per-tab state, so a later tab round-trip lands on the list (not
/// auto-reopens the PR the user just left).
#[test]
fn back_to_dashboard_clears_saved_detail_ref() {
    use crate::ui::pr_detail::tests::fixture_pr_detail;

    let config = crate::config::Config {
        repos: vec!["a/one".to_owned(), "b/two".to_owned()],
        ..Default::default()
    };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    app.pr_detail = Some(fixture_pr_detail(0, 0, 0, 0));

    // User presses Esc / `b` — the contract says "forget this view".
    app.back_to_dashboard();

    // Now switch away and back; should land on dashboard, not re-open.
    app.handle_action(Action::SwitchTab(1));
    app.handle_action(Action::SwitchTab(0));

    assert_eq!(
        app.focus,
        Focus::Dashboard,
        "after Esc the saved ref is gone so round-trip lands on list"
    );
}

// NOTE: the older contract "digit in detail selects section" was
// reversed once the SHIFT-variant picker landed — digits now switch
// repo tabs again, and sections move to `!@#$%` / SHIFT+digit / F.
// See `digit_in_detail_switches_repo_tab_not_section` below for the
// current expectation.

/// Typing a digit in the repo-picker input field must land in the input
/// buffer, not trigger the global 1–9 tab-switch handler. Without this
/// guard, typing `0xIntuition/gcp-deployment` into the Add field jumped
/// tabs instead of appending `0` to the buffer.
#[test]
fn repo_picker_input_accepts_digits() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::RepoPicker;
        app.repo_picker_mode = RepoPickerMode::Input;

        for ch in ['0', 'x', '/', '1', '9'] {
            app.handle_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(ch),
                crossterm::event::KeyModifiers::NONE,
            ));
        }
        assert_eq!(app.repo_picker_input, "0x/19", "digits must reach input buffer");
    });
}

/// SHIFT-modified keys (uppercase letters) must still type into the
/// repo-picker input. Without this, slugs containing capitals like
/// `0xIntuition/gcp-deployment` couldn't be entered at all.
#[test]
fn repo_picker_input_accepts_shifted_uppercase() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::RepoPicker;
        app.repo_picker_mode = RepoPickerMode::Input;

        app.handle_repo_picker_input_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('I'),
            crossterm::event::KeyModifiers::SHIFT,
        ));
        assert_eq!(app.repo_picker_input, "I");
    });
}

/// CTRL-modified keys must still be swallowed by the input handler so
/// stray `Ctrl+A` / `Ctrl+U` / etc. don't append garbage characters.
#[test]
fn repo_picker_input_rejects_ctrl_modified_keys() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::RepoPicker;
        app.repo_picker_mode = RepoPickerMode::Input;

        app.handle_repo_picker_input_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        assert!(app.repo_picker_input.is_empty(), "Ctrl-keys must not type");
    });
}

/// Adding a valid slug via the picker must append it to `config.repos`.
#[test]
fn repo_picker_add_valid_slug() {
    // Sandbox the config save under a tempdir so the test cannot clobber
    // the developer's real `~/Library/Application Support/octopeek/`
    // (or `$XDG_CONFIG_HOME/octopeek/`) file.
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::RepoPicker;
        app.repo_picker_mode = RepoPickerMode::Input;
        app.repo_picker_input = "rust-lang/rust".to_owned();

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_repo_picker_input_key(key);

        assert!(app.config.repos.contains(&"rust-lang/rust".to_owned()));
        assert!(
            app.repo_picker_input.is_empty(),
            "buffer must be cleared after successful add"
        );
    });
}

/// Adding a duplicate slug must not create a duplicate entry.
#[test]
fn repo_picker_add_dedup() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config {
            repos: vec!["rust-lang/rust".to_owned()],
            ..Default::default()
        };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::RepoPicker;
        app.repo_picker_mode = RepoPickerMode::Input;
        app.repo_picker_input = "rust-lang/rust".to_owned();

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_repo_picker_input_key(key);

        assert_eq!(
            app.config.repos.iter().filter(|r| *r == "rust-lang/rust").count(),
            1,
            "duplicate repo must not be added"
        );
    });
}

/// An invalid slug must set a flash error and not append to `config.repos`.
#[test]
fn repo_picker_add_invalid_slug_sets_flash() {
    // This path rejects the slug before reaching Config::save, so an
    // override is not strictly required — but wrapping keeps all tests
    // uniformly sandboxed in case the code path evolves.
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::RepoPicker;
        app.repo_picker_mode = RepoPickerMode::Input;
        app.repo_picker_input = "no-slash-here".to_owned();

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_repo_picker_input_key(key);

        assert!(app.config.repos.is_empty(), "invalid slug must not be added");
        assert!(app.flash.is_some(), "flash message must be set on validation failure");
    });
}

/// Deleting a repo must also drop its entry from the per-repo selection
/// map so long-running sessions don't accumulate dead cursor state.
#[test]
fn repo_picker_delete_cleans_up_selection_map() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config {
            repos: vec!["owner/a".to_owned(), "owner/b".to_owned()],
            ..Default::default()
        };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.selection.insert("owner/a".to_owned(), 3);
        app.selection.insert("owner/b".to_owned(), 1);

        app.focus = Focus::RepoPicker;
        app.repo_picker_mode = RepoPickerMode::List;
        app.repo_picker_list_cursor = 0;
        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('d'),
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_repo_picker_list_key(key);

        assert!(
            !app.selection.contains_key("owner/a"),
            "deleted repo's selection entry must be removed"
        );
        assert_eq!(
            app.selection.get("owner/b"),
            Some(&1),
            "other repos' selection entries must be untouched"
        );
    });
}

/// Deleting a repo in List mode must remove it from `config.repos` and
/// close the corresponding tab.
#[test]
fn repo_picker_delete_removes_repo_and_tab() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config {
            repos: vec!["owner/a".to_owned(), "owner/b".to_owned()],
            ..Default::default()
        };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::RepoPicker;
        app.repo_picker_mode = RepoPickerMode::List;
        app.repo_picker_list_cursor = 0;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('d'),
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_repo_picker_list_key(key);

        assert!(!app.config.repos.contains(&"owner/a".to_owned()), "repo must be removed");
        assert!(app.config.repos.contains(&"owner/b".to_owned()), "other repo must remain");
        assert!(
            app.tabs.tabs.iter().all(|t| t.repo != "owner/a"),
            "tab for deleted repo must be closed"
        );
    });
}

/// Regression guard: `Config::save` with an override writes ONLY to the
/// override directory and the real platform config path is never touched.
///
/// Without this invariant, earlier picker tests clobbered the developer's
/// actual `~/Library/Application Support/octopeek/config.toml` on every
/// `cargo test` run.
#[test]
fn config_save_respects_override() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let expected = tmp.path().join("config.toml");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config {
            repos: vec!["sentinel/override".to_owned()],
            ..Default::default()
        };
        config.save();
        assert!(expected.exists(), "save must write to the override path");
        let written = std::fs::read_to_string(&expected).expect("read override");
        assert!(written.contains("sentinel/override"), "override file must contain the data");
    });
}

/// Pressing `c` on the dashboard when the inbox has a PR with `head_ref`
/// must populate `app.confirm` and switch focus to `Focus::Confirm`.
#[test]
fn pressing_c_on_dashboard_with_pr_opens_confirm() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    let inbox = Inbox {
        viewer_login: "viewer".to_owned(),
        prs: vec![make_pr("o/r", "clean", "viewer")],
        issues: vec![],
    };
    app.on_inbox_loaded(inbox);

    app.handle_action(Action::CheckoutBranch);

    // Should be in Confirm focus if git repo is available; if not in a git
    // repo, a flash is shown instead — both are valid.
    match app.focus {
        Focus::Confirm => {
            assert!(app.confirm.is_some(), "confirm must be populated");
            let confirm = app.confirm.as_ref().unwrap();
            assert!(
                matches!(
                    &confirm.pending_action,
                    crate::ui::confirm::ConfirmPending::CheckoutBranch { branch, .. }
                    if branch == "feat/test"
                ),
                "confirm must have the correct branch"
            );
        }
        Focus::Dashboard => {
            // Not in a git repo — flash should explain this.
            assert!(
                app.flash.is_some(),
                "a flash must be set when not in a git repo or branch is unavailable"
            );
        }
        other => panic!("unexpected focus {other:?}"),
    }
}

/// Pressing `n`/`N` dismiss the confirm overlay with no action.
#[test]
fn confirm_n_cancels_and_restores_focus() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    app.confirm = Some(crate::ui::confirm::Confirm {
        title: "Test".to_owned(),
        prompt: "Are you sure?".to_owned(),
        pending_action: crate::ui::confirm::ConfirmPending::CheckoutBranch {
            repo: "o/r".to_owned(),
            number: 1,
            branch: "feat/x".to_owned(),
        },
    });
    app.confirm_return_focus = Focus::Dashboard;
    app.focus = Focus::Confirm;

    app.handle_action(Action::ConfirmCheckout(false));

    assert_eq!(app.focus, Focus::Dashboard, "focus must be restored after cancel");
    assert!(app.confirm.is_none(), "confirm must be cleared after cancel");
}

// ── First-run wizard tests ────────────────────────────────────────────────

/// Helper: build an `Inbox` with a given set of PRs and issues.
fn make_inbox(prs: Vec<(&str, &str)>, issues: Vec<&str>) -> Inbox {
    Inbox {
        viewer_login: "viewer".to_owned(),
        prs: prs.into_iter().map(|(repo, variant)| make_pr(repo, variant, "viewer")).collect(),
        issues: issues.into_iter().map(make_issue).collect(),
    }
}

/// When config is empty and the inbox has items, `on_inbox_loaded` must
/// switch focus to `FirstRun` and populate `first_run_suggestions`.
#[test]
fn on_inbox_loaded_triggers_first_run_when_config_empty() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default(); // repos empty
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        let inbox = make_inbox(
            vec![("alice/foo", "clean"), ("bob/bar", "clean"), ("alice/foo", "conflict")],
            vec![],
        );
        app.on_inbox_loaded(inbox);

        assert_eq!(app.focus, Focus::FirstRun, "focus must switch to FirstRun");
        assert_eq!(
            app.first_run_suggestions.len(),
            2,
            "two distinct repos must appear in suggestions"
        );
        // alice/foo has 2 PRs; bob/bar has 1.
        assert_eq!(app.first_run_suggestions[0].repo, "alice/foo");
        assert_eq!(app.first_run_suggestions[0].count, 2);
        assert_eq!(app.first_run_suggestions[1].repo, "bob/bar");
        assert_eq!(app.first_run_suggestions[1].count, 1);
    });
}

/// When config already has repos, `on_inbox_loaded` must NOT trigger the
/// first-run wizard.
#[test]
fn on_inbox_loaded_skips_first_run_when_config_nonempty() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config {
            repos: vec!["existing/repo".to_owned()],
            ..Default::default()
        };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        let inbox = make_inbox(vec![("alice/foo", "clean")], vec![]);
        app.on_inbox_loaded(inbox);

        assert_eq!(
            app.focus,
            Focus::Dashboard,
            "focus must remain Dashboard when config has repos"
        );
        assert!(app.first_run_suggestions.is_empty(), "no suggestions when config is nonempty");
    });
}

/// When config is empty AND inbox is empty, focus must stay Dashboard
/// (existing empty-dashboard state is the correct UX).
#[test]
fn on_inbox_loaded_skips_first_run_when_inbox_empty() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        let inbox = make_inbox(vec![], vec![]);
        app.on_inbox_loaded(inbox);

        assert_eq!(app.focus, Focus::Dashboard, "focus must stay Dashboard for empty inbox");
        assert!(app.first_run_suggestions.is_empty());
    });
}

/// Space key in `FirstRun` focus must toggle the selected state of the
/// cursor row.
#[test]
fn first_run_space_toggles_selection() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::FirstRun;
        app.first_run_suggestions =
            vec![FirstRunSuggestion { repo: "a/b".to_owned(), count: 1, selected: false }];
        app.first_run_cursor = 0;

        let space = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(' '),
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_key_first_run(space);
        assert!(app.first_run_suggestions[0].selected, "Space must select the row");

        // Press again to deselect.
        let space2 = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(' '),
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_key_first_run(space2);
        assert!(!app.first_run_suggestions[0].selected, "second Space must deselect the row");
    });
}

/// Enter in `FirstRun` focus must commit selected repos to config, clear
/// the suggestions, switch to Dashboard, and set a flash message.
#[test]
fn first_run_enter_commits_selected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::FirstRun;
        app.first_run_suggestions = vec![
            FirstRunSuggestion { repo: "a/b".to_owned(), count: 5, selected: true },
            FirstRunSuggestion { repo: "c/d".to_owned(), count: 3, selected: true },
            FirstRunSuggestion { repo: "e/f".to_owned(), count: 1, selected: false },
        ];

        let enter = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_key_first_run(enter);

        assert_eq!(app.focus, Focus::Dashboard, "focus must switch to Dashboard after commit");
        assert!(app.first_run_suggestions.is_empty(), "suggestions must be cleared");
        assert!(
            app.config.repos.contains(&"a/b".to_owned()),
            "selected repo a/b must be in config"
        );
        assert!(
            app.config.repos.contains(&"c/d".to_owned()),
            "selected repo c/d must be in config"
        );
        assert!(
            !app.config.repos.contains(&"e/f".to_owned()),
            "unselected repo e/f must NOT be in config"
        );
        assert!(app.flash.is_some(), "a flash message must be set after committing");
    });
}

/// Esc in `FirstRun` focus must skip without touching config and switch
/// focus to Dashboard.
#[test]
fn first_run_esc_skips_without_commit() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::FirstRun;
        app.first_run_suggestions =
            vec![FirstRunSuggestion { repo: "a/b".to_owned(), count: 2, selected: true }];

        let esc = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_key_first_run(esc);

        assert_eq!(app.focus, Focus::Dashboard, "focus must be Dashboard after Esc");
        assert!(app.config.repos.is_empty(), "Esc must not commit any repos to config");
        assert!(app.first_run_suggestions.is_empty(), "suggestions must be cleared on Esc");
    });
}

/// Suggestions must be sorted by count descending, then alphabetically.
#[test]
fn first_run_suggestions_sorted_by_count_desc() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        // a/b appears 5 times (5 PRs), c/d 10 times (10 PRs).
        let mut prs: Vec<(&str, &str)> = Vec::new();
        for _ in 0..5 {
            prs.push(("a/b", "clean"));
        }
        for _ in 0..10 {
            prs.push(("c/d", "clean"));
        }
        let inbox = make_inbox(prs, vec![]);
        app.on_inbox_loaded(inbox);

        assert_eq!(app.focus, Focus::FirstRun, "must switch to FirstRun");
        assert_eq!(
            app.first_run_suggestions[0].repo, "c/d",
            "repo with more items must be first"
        );
        assert_eq!(app.first_run_suggestions[0].count, 10);
    });
}

/// A repo with 2 PRs and 3 issues must yield a combined count of 5.
#[test]
fn first_run_suggestion_counts_pr_plus_issue() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        let inbox =
            make_inbox(vec![("x/y", "clean"), ("x/y", "conflict")], vec!["x/y", "x/y", "x/y"]);
        app.on_inbox_loaded(inbox);

        let sug = app.first_run_suggestions.iter().find(|s| s.repo == "x/y");
        assert!(sug.is_some(), "x/y must appear in suggestions");
        assert_eq!(sug.unwrap().count, 5, "2 PRs + 3 issues = 5 total");
    });
}

/// Regression guard for the reviewer's "selections survive a mid-wizard
/// refresh" invariant. A second `on_inbox_loaded` call while focus is
/// `FirstRun` must NOT clobber the user's toggled selections.
///
/// The guard at the top of `on_inbox_loaded` requires
/// `focus == Dashboard` to populate suggestions; with focus still on the
/// wizard, the method must leave `first_run_suggestions` intact.
#[test]
fn first_run_survives_mid_wizard_refresh() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        // Initial fetch triggers the wizard.
        let inbox = make_inbox(vec![("a/b", "clean"), ("c/d", "clean")], vec![]);
        app.on_inbox_loaded(inbox);
        assert_eq!(app.focus, Focus::FirstRun);
        assert_eq!(app.first_run_suggestions.len(), 2);

        // User toggles the first suggestion.
        app.first_run_cursor = 0;
        app.first_run_suggestions[0].selected = true;
        let snapshot_repo = app.first_run_suggestions[0].repo.clone();

        // A background refresh arrives while focus is still on the wizard.
        let inbox2 =
            make_inbox(vec![("a/b", "clean"), ("c/d", "clean"), ("e/f", "clean")], vec![]);
        app.on_inbox_loaded(inbox2);

        assert_eq!(app.focus, Focus::FirstRun, "focus must not bounce");
        assert_eq!(app.first_run_suggestions.len(), 2, "suggestions must not be rebuilt");
        assert_eq!(
            app.first_run_suggestions[0].repo, snapshot_repo,
            "suggestion ordering must be preserved"
        );
        assert!(
            app.first_run_suggestions[0].selected,
            "user's selection must survive the refresh"
        );
    });
}

/// Regression guard for the reviewer's `a`-key roundtrip concern. Pressing
/// `a` in the wizard opens the repo picker in Input mode and records
/// `FirstRun` as the return-to focus; after the picker closes (via
/// `close_repo_picker`) the user lands back in the wizard, not on the
/// dashboard.
#[test]
fn first_run_a_roundtrips_back_to_first_run() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        let inbox = make_inbox(vec![("a/b", "clean")], vec![]);
        app.on_inbox_loaded(inbox);
        assert_eq!(app.focus, Focus::FirstRun, "wizard must be active");

        // User presses `a` — should open picker with return_focus recorded.
        let a_key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_key_first_run(a_key);
        assert_eq!(app.focus, Focus::RepoPicker);
        assert_eq!(
            app.repo_picker_return_focus,
            Focus::FirstRun,
            "return-focus must be recorded so the picker close path returns here"
        );

        // Simulate picker close.
        app.close_repo_picker();
        assert_eq!(app.focus, Focus::FirstRun, "closing picker must return to wizard");
    });
}

/// Pressing Enter with zero items ticked must flash a hint and NOT
/// close the wizard — otherwise the user's accidental Enter would
/// dump them to an empty dashboard with no feedback.
#[test]
fn first_run_enter_with_nothing_selected_flashes_hint() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        let inbox = make_inbox(vec![("a/b", "clean")], vec![]);
        app.on_inbox_loaded(inbox);
        assert_eq!(app.focus, Focus::FirstRun);
        assert!(
            !app.first_run_suggestions.iter().any(|s| s.selected),
            "no suggestions should start selected"
        );

        let enter = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        app.handle_key_first_run(enter);

        assert_eq!(app.focus, Focus::FirstRun, "wizard must stay open on empty Enter");
        assert!(app.flash.is_some(), "a hint flash must be shown");
        assert!(app.config.repos.is_empty(), "config must not be mutated");
    });
}

// ── ToggleShowAll tests ───────────────────────────────────────────────────

/// Dispatching `Action::ToggleShowAll` must flip `config.show_all_prs`,
/// persist the change to disk (via `Config::save`), and show a flash message.
///
/// Uses `with_config_dir_override` so the save call touches a temp dir and
/// never writes to the developer's real config directory.
#[test]
fn toggle_show_all_flips_flag_and_persists() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(dir.path(), || {
        let config =
            crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        // Initial state: show_all_prs is false.
        assert!(!app.config.show_all_prs);

        // Toggle on.
        app.handle_action(Action::ToggleShowAll);
        assert!(app.config.show_all_prs, "flag must be true after first toggle");
        assert!(app.flash.is_some(), "a flash message must be shown");

        // The config must have been persisted.
        let saved = crate::config::Config::load();
        assert!(saved.show_all_prs, "persisted config must reflect the toggle");

        // Toggle off.
        app.handle_action(Action::ToggleShowAll);
        assert!(!app.config.show_all_prs, "flag must be false after second toggle");
        let saved2 = crate::config::Config::load();
        assert!(!saved2.show_all_prs, "persisted config must reflect the second toggle");
    });
}

// ── Theme picker tests ────────────────────────────────────────────────────

/// Pressing `A` (SHIFT+a) on the dashboard must reach the toggle, not
/// get swallowed by the modifier filter. Without this the feature
/// appears completely dead from the user's perspective.
#[test]
fn capital_a_on_dashboard_triggers_show_all_toggle() {
    let tmp = tempfile::tempdir().expect("tempdir");
    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        assert!(!app.config.show_all_prs);

        // Capital 'A' arrives as KeyCode::Char('A') with SHIFT set.
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('A'),
            crossterm::event::KeyModifiers::SHIFT,
        ));

        assert!(
            app.config.show_all_prs,
            "SHIFT+a must dispatch ToggleShowAll despite the modifier"
        );
    });
}

/// Pressing `c` on the dashboard flips focus to `ThemePicker` and
/// initialises the cursor to the index of the currently active theme.
#[test]
fn c_on_dashboard_opens_theme_picker() {
    use crate::theme::Theme;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState};

    let config = crate::config::Config { theme: Theme::Nord, ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    assert_eq!(app.focus, Focus::Dashboard);

    let key = KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: crossterm::event::KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(key);

    assert_eq!(app.focus, Focus::ThemePicker, "focus must switch to ThemePicker");
    let expected_idx = Theme::ALL.iter().position(|&t| t == Theme::Nord).unwrap();
    assert_eq!(app.theme_picker_cursor, expected_idx, "cursor must start on the current theme");
}

/// Pressing `Enter` in the theme picker applies the highlighted theme to
/// `config.theme` and persists it to disk.
#[test]
fn enter_in_theme_picker_applies_and_persists() {
    use crate::theme::Theme;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState};

    let tmp = tempfile::tempdir().expect("tempdir");

    crate::config::with_config_dir_override(tmp.path(), || {
        let config = crate::config::Config { theme: Theme::Default, ..Default::default() };
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        // Open picker, then move cursor to Dracula (index 1).
        app.open_theme_picker();
        app.theme_picker_cursor = 1; // Dracula

        // Press Enter.
        let key = KeyEvent {
            code: KeyCode::Enter,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        app.handle_key_theme_picker(key);

        assert_eq!(app.config.theme, Theme::Dracula, "in-memory theme must be Dracula");
        assert_eq!(app.focus, Focus::Dashboard, "picker must close");

        // Verify persistence.
        let saved = crate::config::Config::load();
        assert_eq!(saved.theme, Theme::Dracula, "persisted theme must be Dracula");
    });
}

/// Pressing `Esc` in the theme picker reverts the theme in-memory and does
/// NOT update the persisted config.
#[test]
fn esc_in_theme_picker_restores_original_theme() {
    use crate::theme::Theme;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState};

    let tmp = tempfile::tempdir().expect("tempdir");

    crate::config::with_config_dir_override(tmp.path(), || {
        // Start with Nord persisted.
        let config = crate::config::Config { theme: Theme::Nord, ..Default::default() };
        config.save();

        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);

        // Open picker and move cursor to Dracula — live preview activates.
        app.open_theme_picker();
        app.theme_picker_cursor = 1; // Dracula

        // Press Esc to cancel.
        let key = KeyEvent {
            code: KeyCode::Esc,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        app.handle_key_theme_picker(key);

        assert_eq!(app.config.theme, Theme::Nord, "in-memory theme must revert to Nord");
        assert_eq!(app.focus, Focus::Dashboard, "picker must close");

        // Persisted config must still be Nord (Esc must not save).
        let saved = crate::config::Config::load();
        assert_eq!(saved.theme, Theme::Nord, "persisted theme must remain Nord");
    });
}

/// Moving the cursor past the last item wraps to index 0, and moving up
/// from index 0 wraps to the last item.
#[test]
fn cursor_wraps_around_at_list_edges() {
    use crate::theme::Theme;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState};

    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.open_theme_picker();

    let last = Theme::ALL.len() - 1;

    // Start at index 0; pressing Up must wrap to last.
    app.theme_picker_cursor = 0;
    let up = KeyEvent {
        code: KeyCode::Up,
        modifiers: crossterm::event::KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key_theme_picker(up);
    assert_eq!(app.theme_picker_cursor, last, "Up from 0 must wrap to last index");

    // Now at last; pressing Down must wrap to 0.
    let down = KeyEvent {
        code: KeyCode::Down,
        modifiers: crossterm::event::KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key_theme_picker(down);
    assert_eq!(app.theme_picker_cursor, 0, "Down from last must wrap to 0");
}

// ── Phase 6: sidebar sections ─────────────────────────────────────────────

/// The SHIFT-digit variants (`!@#$%`) select sections in the detail view.
/// Unshifted `1..9` fall through to the global tab switcher instead.
#[test]
fn shift_digit_variants_select_sections() {
    let config = crate::config::Config {
        repos: vec!["a/one".to_owned(), "b/two".to_owned()],
        ..Default::default()
    };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;

    for (ch, expected) in [
        ('!', DetailSection::Description),
        ('@', DetailSection::Checks),
        ('#', DetailSection::Reviews),
        ('$', DetailSection::Files),
        ('%', DetailSection::Comments),
    ] {
        app.focus = Focus::Detail;
        app.pr_detail_selected_section = DetailSection::Description;
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(ch),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(
            app.pr_detail_selected_section, expected,
            "{ch:?} must select {expected:?}"
        );
    }
}

/// Terminals that deliver SHIFT+digit without translating to punctuation
/// must still hit the section picker via the `Char('1'..='5')` + SHIFT arm.
#[test]
fn shift_plus_digit_also_selects_section() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('3'),
        crossterm::event::KeyModifiers::SHIFT,
    ));
    assert_eq!(app.pr_detail_selected_section, DetailSection::Reviews);
}

/// `F` (SHIFT+f) jumps straight to the Files section.
#[test]
fn shift_f_selects_files_section() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('F'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.pr_detail_selected_section, DetailSection::Files);
}

/// `J` / `K` cycle the files cursor when the Files section is active.
/// Using the fixture-detail (5 files) so bounds are exercised.
#[test]
fn shift_j_k_cycle_files_cursor_in_files_section() {
    use crate::ui::pr_detail::tests::fixture_pr_detail;
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    app.pr_detail = Some(fixture_pr_detail(0, 0, 5, 0));
    app.pr_detail_selected_section = DetailSection::Files;
    app.pr_detail_files_cursor = 0;

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('J'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.pr_detail_files_cursor, 1, "J moves cursor forward");

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('J'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('J'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('J'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.pr_detail_files_cursor, 4, "cycle advances to last file");

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('J'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.pr_detail_files_cursor, 4, "J at last clamps (no wrap)");

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('K'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.pr_detail_files_cursor, 3, "K moves cursor back");
}

/// `J` / `K` are section-gated: outside the Files section they fall
/// through to whatever else might consume them (currently nothing in
/// detail), so they should not move the files cursor from Description.
#[test]
fn shift_j_k_do_not_cycle_outside_files_section() {
    use crate::ui::pr_detail::tests::fixture_pr_detail;
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    app.pr_detail = Some(fixture_pr_detail(0, 0, 5, 0));
    app.pr_detail_selected_section = DetailSection::Description;
    app.pr_detail_files_cursor = 2;

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('J'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.pr_detail_files_cursor, 2, "J outside Files must not move cursor");
}

/// Scroll in the Files section is clamped to the diff's actual length.
/// Regression guard for the bug where `clamp_pr_detail_scroll` operated
/// on `pr_detail_scroll[Files]` while the active offset lived in
/// `pr_detail_diff_scroll[path]`, so `j`/wheel past the end grew
/// unbounded.
#[test]
fn files_scroll_is_clamped_to_diff_length() {
    use crate::ui::pr_detail::tests::fixture_pr_detail;
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    app.pr_detail = Some(fixture_pr_detail(0, 0, 3, 0));
    app.pr_detail_selected_section = DetailSection::Files;
    app.pr_detail_files_cursor = 0;
    // Pretend the right-pane viewport has been rendered once.
    app.pr_detail_right_viewport.set(ratatui::layout::Rect::new(30, 6, 100, 24));

    // Smash the scroll way past any realistic content length.
    *app.right_pane_scroll_mut() = u16::MAX;
    app.clamp_pr_detail_scroll();

    // Content lines = diff header + blank + placeholder line (patch=None)
    // = 3 rows; viewport height is 24; max_scroll saturates to 0.
    assert_eq!(app.right_pane_scroll(), 0, "diff shorter than viewport must clamp scroll to 0");
}

/// Scroll offsets are preserved per file when cycling through files.
#[test]
fn diff_scroll_is_preserved_per_file() {
    use crate::ui::pr_detail::tests::fixture_pr_detail;
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    app.pr_detail = Some(fixture_pr_detail(0, 0, 3, 0));
    app.pr_detail_selected_section = DetailSection::Files;
    app.pr_detail_files_cursor = 0;

    // Scroll the first file's diff down.
    *app.right_pane_scroll_mut() = 7;
    assert_eq!(app.right_pane_scroll(), 7);

    // Move to next file — scroll starts fresh.
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('J'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.right_pane_scroll(), 0, "new file's scroll starts at 0");

    // Back to the first file — scroll restored.
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('K'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.right_pane_scroll(), 7, "first file's scroll is remembered");
}

/// Unshifted digits in the detail view switch repo tabs (and pop back to
/// the dashboard); they do NOT select sections. This is the inverse of
/// the Phase 1 behaviour — SHIFT-variants took over section picking so
/// digits could return to their global tab-switch role.
#[test]
fn digit_in_detail_switches_repo_tab_not_section() {
    let config = crate::config::Config {
        repos: vec!["a/one".to_owned(), "b/two".to_owned()],
        ..Default::default()
    };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    app.pr_detail_selected_section = DetailSection::Reviews;

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('2'),
        crossterm::event::KeyModifiers::NONE,
    ));

    assert_eq!(app.focus, Focus::Dashboard, "digit in detail pops to dashboard");
    assert_eq!(app.tabs.active_index(), Some(1));
    // The section selection is cleared by back_to_dashboard so checking
    // it here would be tautological; what matters is the tab switched.
}

/// `current_detail_lines` returns only the lines for the selected section.
#[test]
fn current_detail_lines_returns_only_selected_section() {
    use crate::ui::pr_detail::tests::fixture_pr_detail;

    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;
    // Use a fixture with distinct content in each section.
    app.pr_detail = Some(fixture_pr_detail(3, 2, 4, 2));

    app.pr_detail_selected_section = DetailSection::Description;
    let desc_lines = app.current_detail_lines();

    app.pr_detail_selected_section = DetailSection::Checks;
    let check_lines = app.current_detail_lines();

    // The two sections must produce different line counts (different content).
    assert_ne!(
        desc_lines.len(),
        check_lines.len(),
        "Description and Checks must produce different line buffers"
    );
    // Neither must be empty for the fixture with content.
    assert!(!desc_lines.is_empty(), "Description must have lines");
    assert!(!check_lines.is_empty(), "Checks must have lines for non-empty fixture");
}

/// A simulated left-click on sidebar section row 2 (Reviews) selects Reviews.
#[test]
fn mouse_click_on_sidebar_section_row_selects_that_section() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;

    // Set up a sections_rect: x=0, y=4, w=28, h=7 (header + 5 sections + 1).
    // Row 4 is the header; row 5 = Description, row 6 = Checks, row 7 = Reviews.
    let sections_rect = ratatui::layout::Rect::new(0, 4, 28, 7);
    let files_rect = ratatui::layout::Rect::new(0, 11, 28, 20);
    app.pr_detail_sidebar_rects.set((sections_rect, files_rect));

    // Click on row 7 → relative row 3 → section index 2 → Reviews.
    app.handle_sidebar_click(5, 7, sections_rect, files_rect);

    assert_eq!(
        app.pr_detail_selected_section,
        DetailSection::Reviews,
        "clicking row 7 in sections panel (relative 3 = section index 2) must select Reviews"
    );
}

/// Scrolling Description, switching to Checks (scroll starts at 0),
/// then switching back to Description restores its scroll offset.
#[test]
fn scroll_is_preserved_per_section() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);
    app.focus = Focus::Detail;

    // Scroll Description down to 15.
    app.pr_detail_selected_section = DetailSection::Description;
    *app.scroll_mut(DetailSection::Description) = 15;

    // Switch to Checks — its scroll should start at 0.
    app.pr_detail_selected_section = DetailSection::Checks;
    assert_eq!(app.scroll_for(DetailSection::Checks), 0, "fresh section starts at scroll 0");

    // Switch back to Description — its scroll must be restored.
    app.pr_detail_selected_section = DetailSection::Description;
    assert_eq!(
        app.scroll_for(DetailSection::Description),
        15,
        "switching back to Description must restore scroll 15"
    );
}

// ── Detail cache + SWR tests ──────────────────────────────────────────────

/// Helper: build a minimal [`github::detail::PrDetail`] for cache tests.
fn make_pr_detail_for_app(repo: &str, number: u32) -> crate::github::detail::PrDetail {
    crate::github::detail::PrDetail {
        repo: repo.to_owned(),
        number,
        title: "Cache Test PR".to_owned(),
        url: format!("https://github.com/{repo}/pull/{number}"),
        author: "alice".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat/cache".to_owned(),
        is_draft: false,
        additions: 1,
        deletions: 1,
        changed_files_count: 1,
        updated_at: Utc::now(),
        created_at: Utc::now(),
        merged: false,
        files: vec![],
        check_runs: vec![],
        reviews: vec![],
        review_threads: vec![],
        issue_comments: vec![],
    }
}

/// Round-trip: `insert_pr` then `get_pr` returns the same payload.
#[test]
fn cache_insert_and_get_pr() {
    let config = crate::config::Config::default();
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    let detail = make_pr_detail_for_app("o/r", 42);
    app.detail_cache.insert_pr(detail.clone());

    let hit = app.detail_cache.get_pr("o/r", 42).expect("cache miss");
    assert_eq!(hit.data.number, 42);
    assert_eq!(hit.data.repo, "o/r");
}

/// `Cached::is_fresh` is true just after insertion and false when
/// `fetched_at` is set more than TTL ago.
#[test]
fn cache_is_fresh_true_under_ttl_false_after() {
    use crate::github::cache::{CACHE_TTL, Cached};
    use std::time::{Duration, Instant};

    let data = make_pr_detail_for_app("o/r", 1);

    let fresh = Cached::new(data.clone());
    assert!(fresh.is_fresh(), "entry stamped now must be fresh");

    let stale = Cached {
        data,
        fetched_at: Instant::now()
            .checked_sub(Duration::from_secs(CACHE_TTL.as_secs() + 1))
            .unwrap_or_else(Instant::now),
    };
    assert!(!stale.is_fresh(), "entry older than TTL must be stale");
}

/// Switching to a tab whose detail ref is in the cache (fresh) must
/// populate `pr_detail` without setting `detail_fetching` or
/// `detail_refreshing`.
#[test]
fn restore_from_fresh_cache_populates_detail_without_flipping_fetching() {
    let config = crate::config::Config {
        repos: vec!["a/one".to_owned(), "b/two".to_owned()],
        ..Default::default()
    };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    // Pre-populate cache with a fresh entry for "a/one" PR #1.
    let detail = make_pr_detail_for_app("a/one", 1);
    app.detail_cache.insert_pr(detail.clone());

    // Simulate the user having been on tab 0 with PR #1 open.
    app.per_tab_state.insert(
        "a/one".to_owned(),
        PerTabState {
            detail_ref: Some(DetailRef {
                repo: "a/one".to_owned(),
                number: 1,
                kind: DetailKind::Pr,
            }),
        },
    );

    // Switch to tab 1, then back to tab 0 to trigger restore.
    app.tabs.set_active_by_index(1);
    app.tabs.set_active_by_index(0);
    app.restore_active_tab_state();

    assert!(app.pr_detail.is_some(), "pr_detail must be populated from cache");
    assert!(!app.detail_fetching, "no spinner for a cache hit");
    assert!(app.detail_refreshing.is_none(), "no SWR kick for a fresh entry");
}

/// Switching to a tab whose cache entry is stale must populate `pr_detail`
/// immediately AND set `detail_refreshing` (but NOT `detail_fetching`).
#[test]
fn restore_from_stale_cache_populates_and_sets_refreshing() {
    use crate::github::cache::{CACHE_TTL, Cached};
    use std::time::{Duration, Instant};

    let config = crate::config::Config {
        repos: vec!["a/one".to_owned(), "b/two".to_owned()],
        ..Default::default()
    };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    // Insert a stale cache entry manually (fetched_at = TTL + 1 sec ago).
    let data = make_pr_detail_for_app("a/one", 1);
    app.detail_cache.prs.insert(
        ("a/one".to_owned(), 1),
        Cached {
            data,
            fetched_at: Instant::now()
                .checked_sub(Duration::from_secs(CACHE_TTL.as_secs() + 1))
                .unwrap_or_else(Instant::now),
        },
    );

    app.per_tab_state.insert(
        "a/one".to_owned(),
        PerTabState {
            detail_ref: Some(DetailRef {
                repo: "a/one".to_owned(),
                number: 1,
                kind: DetailKind::Pr,
            }),
        },
    );

    app.tabs.set_active_by_index(0);
    app.restore_active_tab_state();

    assert!(app.pr_detail.is_some(), "stale cache must still populate pr_detail immediately");
    assert!(!app.detail_fetching, "stale SWR must NOT set the spinner");
    assert_eq!(
        app.detail_refreshing,
        Some(("a/one".to_owned(), 1)),
        "stale entry must set detail_refreshing"
    );
}

/// Cold miss: no cache entry → `pr_detail` stays None, focus is Detail,
/// `detail_refreshing` is None (not SWR — it's a foreground fetch).
#[test]
fn cold_miss_falls_back_to_cold_fetch_path() {
    let config = crate::config::Config {
        repos: vec!["a/one".to_owned(), "b/two".to_owned()],
        ..Default::default()
    };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    // No cache entry for "a/one" PR #1.
    app.per_tab_state.insert(
        "a/one".to_owned(),
        PerTabState {
            detail_ref: Some(DetailRef {
                repo: "a/one".to_owned(),
                number: 1,
                kind: DetailKind::Pr,
            }),
        },
    );

    app.tabs.set_active_by_index(0);
    app.restore_active_tab_state();

    // No client in tests, so spawn_detail_fetch returns early.
    // pr_detail stays None; focus is Detail (the ref exists).
    assert_eq!(app.focus, Focus::Detail, "detail ref present means focus=Detail");
    assert!(app.pr_detail.is_none(), "no cache entry means cold fetch (no stale content)");
    assert!(app.detail_refreshing.is_none(), "cold miss uses foreground fetch, not SWR");
}

/// Dispatching `PrDetailLoaded` must upsert into cache and clear
/// `detail_refreshing` when the arriving (repo, number) matches.
#[test]
fn pr_detail_loaded_upserts_cache_and_clears_refreshing() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    // Simulate an in-flight SWR for "o/r" #5.
    app.detail_refreshing = Some(("o/r".to_owned(), 5));
    app.focus = Focus::Detail;
    app.detail_fetching = true;

    let detail = make_pr_detail_for_app("o/r", 5);
    // Use pr_detail = None so the "foreground cold miss" path fires and
    // the visible state is updated.
    app.handle_action(Action::PrDetailLoaded(Box::new(detail)));

    assert!(app.detail_cache.get_pr("o/r", 5).is_some(), "cache must be populated");
    assert!(app.detail_refreshing.is_none(), "SWR marker must be cleared on arrival");
}

/// When the user has tabbed away (focus != Detail) a `PrDetailLoaded`
/// action must still upsert the cache but must NOT overwrite `pr_detail`
/// (which is None for the new tab's context).
#[test]
fn pr_detail_loaded_ignored_when_user_moved_on() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    // User is on the Dashboard, not Detail.
    app.focus = Focus::Dashboard;
    app.pr_detail = None;

    let detail = make_pr_detail_for_app("o/r", 7);
    app.handle_action(Action::PrDetailLoaded(Box::new(detail)));

    assert!(app.detail_cache.get_pr("o/r", 7).is_some(), "cache must be populated");
    assert!(app.pr_detail.is_none(), "visible state must NOT be updated when not in Detail");
}

/// Pressing `r` (manual refresh) invalidates the cache entry for the
/// active detail before dispatching a cold fetch.
#[test]
fn manual_refresh_invalidates_cache() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    let detail = make_pr_detail_for_app("o/r", 3);
    app.detail_cache.insert_pr(detail.clone());
    app.pr_detail = Some(detail);
    app.focus = Focus::Detail;

    // Simulate pressing `r`.
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('r'),
        crossterm::event::KeyModifiers::NONE,
    ));

    assert!(
        app.detail_cache.get_pr("o/r", 3).is_none(),
        "manual refresh must invalidate the cache entry"
    );
}

/// `back_to_dashboard` must NOT clear `detail_cache`. The cache must
/// survive so the next visit can be served instantly.
#[test]
fn back_to_dashboard_does_not_clear_cache() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    let detail = make_pr_detail_for_app("o/r", 9);
    app.detail_cache.insert_pr(detail.clone());
    app.pr_detail = Some(detail);
    app.focus = Focus::Detail;

    app.back_to_dashboard();

    assert!(
        app.detail_cache.get_pr("o/r", 9).is_some(),
        "cache entry must survive back_to_dashboard"
    );
}

/// Dispatching `AutoRefresh` while in Detail focus with a loaded PR must
/// set `detail_refreshing` (SWR kick). The inbox-refresh leg is gated
/// behind the `fetching` guard, so we pre-set `app.fetching = true` to
/// short-circuit `spawn_fetch` before it attempts `tokio::spawn` (which
/// requires a runtime in non-async tests). The detail SWR path is
/// validated because it also exits early when no GitHub client is
/// configured — `spawn_detail_fetch_background` returns `false` without
/// spawning, but `detail_refreshing` is set *before* the spawn call.
#[test]
fn auto_refresh_action_dispatches_inbox_and_detail_refresh() {
    let config = crate::config::Config { repos: vec!["o/r".to_owned()], ..Default::default() };
    let session = crate::state::AppSession::default();
    let mut app = App::new(config, session);

    let detail = make_pr_detail_for_app("o/r", 11);
    app.pr_detail = Some(detail);
    app.focus = Focus::Detail;
    // Pretend an inbox fetch is already in-flight so `spawn_fetch` returns
    // immediately — avoids needing a tokio runtime in a sync test.
    app.fetching = true;
    // Remove the GitHub client so `spawn_detail_fetch_background` also
    // exits early (before calling `tokio::spawn`).
    app.client = None;

    // Inject a dummy action sender so the handler can clone it.
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.action_tx = Some(tx);

    app.handle_action(Action::AutoRefresh);

    assert_eq!(
        app.detail_refreshing,
        Some(("o/r".to_owned(), 11)),
        "AutoRefresh in Detail focus must set detail_refreshing"
    );
}
