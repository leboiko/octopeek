//! UI rendering: one `draw` function composes all panels for a single frame.

pub mod confirm;
pub mod copy_mode;
pub mod dashboard;
pub mod diff;
pub mod first_run;
pub mod glyphs;
pub mod help;
pub mod issue_detail;
pub mod markdown;
pub mod pr_detail;
pub mod repo_picker;
pub mod status_bar;
pub mod tab_bar;
pub mod tabs;
pub mod theme_picker;
pub mod util;

use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::Style,
    widgets::Block,
};

/// Render the full application UI for one frame.
///
/// Layout (top to bottom):
/// 1. Tab bar (1 row, only when tabs are open)
/// 2. Main content area (fills remaining height)
/// 3. Status bar (1 row)
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();

    // Paint every cell with the theme background before any widget renders.
    // Without this, cells not covered by a widget retain the terminal's default
    // background, making light themes look broken on dark terminals.
    f.render_widget(Block::default().style(Style::default().bg(app.palette.background)), area);

    let has_tabs = !app.tabs.is_empty();
    let tab_bar_height: u16 = u16::from(has_tabs);

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(tab_bar_height),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    // ── Tab bar ───────────────────────────────────────────────────────────────
    if has_tabs {
        tab_bar::draw(f, app, outer[0]);
    }

    // ── Main content area ─────────────────────────────────────────────────────
    let content_area = outer[1];

    // Route to the appropriate panel based on current focus.
    // RepoPicker and Confirm focus states draw the dashboard beneath them;
    // the overlay is rendered after the main content area.
    match app.focus {
        Focus::Detail => {
            // If PR detail is populated (or being fetched/errored), render it.
            // If issue detail is populated instead, render the issue detail.
            // Fall back to dashboard if neither is populated and no fetch is active.
            if app.pr_detail.is_some()
                || (app.detail_fetching && app.issue_detail.is_none())
                || (app.detail_error.is_some() && app.issue_detail.is_none())
            {
                pr_detail::draw(f, app, content_area);
            } else if app.issue_detail.is_some()
                || app.detail_fetching
                || app.detail_error.is_some()
            {
                issue_detail::draw(f, app, content_area);
            } else {
                // Defensive fallback: both are None and no active fetch.
                dashboard::draw(f, app, content_area);
            }
        }
        _ => {
            dashboard::draw(f, app, content_area);
        }
    }

    // ── Status bar ────────────────────────────────────────────────────────────
    status_bar::draw(f, app, app.flash.as_ref(), outer[2]);

    // ── Overlays (drawn last so they float above everything) ──────────────────
    if app.show_help {
        help::draw(f, app);
    }

    if app.focus == Focus::FirstRun {
        first_run::draw(f, app);
    }

    if app.focus == Focus::RepoPicker {
        repo_picker::draw(f, app);
    }

    if app.focus == Focus::Confirm && app.confirm.is_some() {
        confirm::draw(f, app);
    }

    if app.focus == Focus::ThemePicker {
        theme_picker::draw(f, app);
    }
}
