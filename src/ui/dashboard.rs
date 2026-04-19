//! Dashboard panel: the main PR / issue inbox list.
//!
//! Renders the scrollable list with role glyphs (A/R/@), status indicators,
//! CI column, and a selection cursor backed by `App::selection`.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};

use crate::app::App;
use crate::github::flags::ActionFlag;
use crate::github::types::{CheckState, Issue, PullRequest, Role};
use crate::state::ViewMode;
use crate::ui::glyphs;
use crate::ui::util::{humanize_delta, truncate};

// ── Column layout helpers ─────────────────────────────────────────────────────
//
// Keeping the widths in `const`s — instead of duplicating them in `pr_row` —
// guarantees the layout constraints and the title-truncation arithmetic stay
// in sync when a column is added, removed, or resized.

const PR_ST_WIDTH: u16 = 4;
const PR_COMMENTS_WIDTH: u16 = 5;
const PR_CI_WIDTH: u16 = 7;
const PR_COMMITS_WIDTH: u16 = 4;
const PR_UPDATED_WIDTH: u16 = 14;

/// Compute column `Constraint`s for the PR list based on terminal width.
///
/// Column set priorities (never drop St, Title, CI):
/// - >= 120: St, Title, Comments, CI, Commits, Updated
/// - 100–119: drop Commits
/// - 80–99: drop Commits + Updated
/// - < 80: St, Title, CI
pub fn pr_columns(width: u16) -> Vec<Constraint> {
    pr_layout(width).iter().map(|c| c.constraint).collect()
}

/// Sum of fixed column widths for the layout chosen at `width`. Used by
/// `pr_row` to budget the flex `Title` column so the two places that depend
/// on the column set can never drift out of sync.
fn pr_fixed_cols_width(width: u16) -> u16 {
    pr_layout(width).iter().map(|c| c.fixed_width).sum()
}

/// One column in the PR layout: its layout constraint and, for fixed-width
/// columns, how much horizontal space it consumes. `Fill(1)` columns
/// contribute `0` — their size is whatever's left after the fixed columns.
struct PrColumn {
    constraint: Constraint,
    fixed_width: u16,
}

fn pr_layout(width: u16) -> Vec<PrColumn> {
    let st = PrColumn { constraint: Constraint::Length(PR_ST_WIDTH), fixed_width: PR_ST_WIDTH };
    let title = PrColumn { constraint: Constraint::Fill(1), fixed_width: 0 };
    let comments = PrColumn {
        constraint: Constraint::Length(PR_COMMENTS_WIDTH),
        fixed_width: PR_COMMENTS_WIDTH,
    };
    let ci = PrColumn { constraint: Constraint::Length(PR_CI_WIDTH), fixed_width: PR_CI_WIDTH };
    let commits = PrColumn {
        constraint: Constraint::Length(PR_COMMITS_WIDTH),
        fixed_width: PR_COMMITS_WIDTH,
    };
    let updated = PrColumn {
        constraint: Constraint::Length(PR_UPDATED_WIDTH),
        fixed_width: PR_UPDATED_WIDTH,
    };

    if width >= 120 {
        vec![st, title, comments, ci, commits, updated]
    } else if width >= 100 {
        vec![st, title, comments, ci, updated]
    } else if width >= 80 {
        vec![st, title, comments, ci]
    } else {
        vec![st, title, ci]
    }
}

/// Compute column `Constraint`s for the Issue list based on terminal width.
///
/// - >= 100: #(6), Title(flex), Comments(5), Updated(14), Labels(flex capped ~20)
/// - 70–99: drop Labels
/// - < 70: drop Updated + Labels
fn issue_columns(width: u16) -> Vec<Constraint> {
    if width >= 100 {
        vec![
            Constraint::Length(6),  // Issue number
            Constraint::Fill(1),    // Title
            Constraint::Length(5),  // #comments
            Constraint::Length(14), // Updated
            Constraint::Max(20),    // Labels
        ]
    } else if width >= 70 {
        vec![
            Constraint::Length(6),  // Issue number
            Constraint::Fill(1),    // Title
            Constraint::Length(5),  // #comments
            Constraint::Length(14), // Updated
        ]
    } else {
        vec![
            Constraint::Length(6), // Issue number
            Constraint::Fill(1),   // Title
            Constraint::Length(5), // #comments
        ]
    }
}

// ── Row builders ──────────────────────────────────────────────────────────────

/// Priority-order the viewer's roles: Author > Reviewer > Assignee.
fn primary_role(roles: &[Role]) -> Role {
    if roles.contains(&Role::Author) {
        Role::Author
    } else if roles.contains(&Role::Reviewer) {
        Role::Reviewer
    } else {
        // Default to Assignee even when the roles slice is empty, as a safe fallback.
        Role::Assignee
    }
}

/// Build the 4-char "St cluster" string for a PR row.
///
/// Layout: `{role}{space}{needs_dot}{flag_glyph}`
/// For Draft the `needs_dot` column is blank because a draft is not "needs action".
fn st_cluster(pr: &PullRequest, viewer_login: &str, ascii: bool) -> String {
    let role_ch = glyphs::role_glyph(primary_role(&pr.roles));
    let flag = pr.primary_flag(viewer_login);
    let needs_dot = if flag != ActionFlag::Clean && flag != ActionFlag::Draft {
        if ascii { glyphs::NEEDS_ACTION_ASCII } else { glyphs::NEEDS_ACTION }
    } else {
        ' '
    };
    let (flag_ch, _) = glyphs::flag_glyph(flag, ascii);
    format!("{role_ch} {needs_dot}{flag_ch}")
}

/// Build a `Row` for one pull request.
///
/// Cell count must match the length of `pr_columns(width)`.
fn pr_row<'a>(
    pr: &'a PullRequest,
    viewer_login: &str,
    width: u16,
    ascii: bool,
    p: &crate::theme::Palette,
) -> Row<'a> {
    let flag = pr.primary_flag(viewer_login);
    let (ci_ch, ci_role) = glyphs::ci_glyph(pr.check_state, ascii);
    let (_, flag_role) = glyphs::flag_glyph(flag, ascii);
    let st_color = p.color_for(flag_role);
    let st_text = st_cluster(pr, viewer_login, ascii);

    // CI column text (7 chars wide).
    let ci_text: String = match pr.check_state {
        Some(CheckState::Failure | CheckState::Error) => {
            let n = pr.failing_checks.len();
            // Format: "✖ Nf    " (7 chars total).
            format!("{ci_ch} {n}f    ")
        }
        Some(CheckState::Pending) => format!("{ci_ch} ...   "),
        // Success, Expected, and no-CI all show just the glyph with padding.
        Some(CheckState::Success | CheckState::Expected) | None => format!("{ci_ch}       "),
    };

    let cols = pr_columns(width);
    let col_count = cols.len();

    let mut cells: Vec<Cell<'static>> = Vec::with_capacity(col_count);

    // Col 0: St cluster (always present).
    cells.push(Cell::from(st_text).style(Style::default().fg(st_color)));

    // Col 1: Title (always present).
    let draft_prefix = if pr.is_draft { "[D] " } else { "" };
    let raw_title = format!("{draft_prefix}{}", pr.title);
    let title_width = usize::from(width.saturating_sub(pr_fixed_cols_width(width)));
    let title_text = truncate(&raw_title, title_width.max(6));
    cells.push(Cell::from(title_text));

    // For 4–6 col layouts, insert comments before CI.
    if col_count >= 4 {
        cells.push(
            Cell::from(format!("{:>4} ", pr.comments_count)).style(Style::default().fg(p.muted)),
        );
    }

    // CI col (always present).
    cells.push(Cell::from(ci_text).style(Style::default().fg(p.color_for(ci_role))));

    if col_count == 6 {
        // Commits col (4 wide).
        cells.push(
            Cell::from(format!("{:>3} ", pr.commits_count)).style(Style::default().fg(p.muted)),
        );
        // Updated col (14 wide).
        cells.push(
            Cell::from(truncate(&humanize_delta(&pr.updated_at), 13))
                .style(Style::default().fg(p.muted)),
        );
    } else if col_count == 5 {
        // Updated col only (no Commits).
        cells.push(
            Cell::from(truncate(&humanize_delta(&pr.updated_at), 13))
                .style(Style::default().fg(p.muted)),
        );
    }

    Row::new(cells)
}

/// Build a `Row` for one issue.
///
/// Cell count must match the length of `issue_columns(width)`.
fn issue_row<'a>(issue: &'a Issue, width: u16, p: &crate::theme::Palette) -> Row<'a> {
    let cols = issue_columns(width);
    let col_count = cols.len();

    let mut cells: Vec<Cell<'static>> = Vec::with_capacity(col_count);

    // Issue number (6 wide).
    cells.push(Cell::from(format!("#{:<5}", issue.number)).style(Style::default().fg(p.muted)));

    // Title (flex).
    let fixed: u16 = match col_count {
        5 => 6 + 5 + 14 + 20,
        4 => 6 + 5 + 14,
        _ => 6 + 5,
    };
    let title_width = (width.saturating_sub(fixed)) as usize;
    cells.push(Cell::from(truncate(&issue.title, title_width.max(6))));

    if col_count >= 3 {
        cells.push(
            Cell::from(format!("{:>4} ", issue.comments_count)).style(Style::default().fg(p.muted)),
        );
    }

    if col_count >= 4 {
        cells.push(
            Cell::from(truncate(&humanize_delta(&issue.updated_at), 13))
                .style(Style::default().fg(p.muted)),
        );
    }

    if col_count >= 5 {
        let label_str: String =
            issue.labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(" ");
        cells.push(Cell::from(truncate(&label_str, 19)));
    }

    Row::new(cells)
}

// ── Centered paragraph helper ─────────────────────────────────────────────────

fn centered_message(text: String, style: Style) -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(text, style))).alignment(Alignment::Center)
}

// ── Main draw function ────────────────────────────────────────────────────────

/// Render the dashboard panel into `area`.
///
/// Dispatches to the appropriate sub-renderer based on `App` state:
/// empty repos, auth missing, first load, or the normal list view.
#[allow(clippy::too_many_lines)]
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let p = &app.palette;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(p.border_style())
        .style(Style::default().bg(p.background));

    // ── A. No tabs / empty repos list ─────────────────────────────────────────
    if app.tabs.is_empty() {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No repositories tracked yet. Press `p` to add one.",
                Style::default().fg(p.foreground),
            )),
        ])
        .block(block)
        .alignment(Alignment::Center);
        f.render_widget(msg, area);
        return;
    }

    // ── B. Auth missing ───────────────────────────────────────────────────────
    if app.client.is_none() {
        let err_text = app
            .last_fetch_error
            .clone()
            .unwrap_or_else(|| "GitHub authentication is not configured.".to_owned());

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(format!("  {err_text}"), Style::default().fg(p.danger))),
            Line::from(""),
            Line::from(Span::styled(
                "  Fix: set GITHUB_TOKEN or run `gh auth login`, then press `r`.",
                Style::default().fg(p.muted),
            )),
        ];
        let msg = Paragraph::new(lines).block(block).alignment(Alignment::Left);
        f.render_widget(msg, area);
        return;
    }

    // ── C. First load in progress ─────────────────────────────────────────────
    if app.fetching && app.inbox.is_none() {
        let msg =
            centered_message("Fetching pull requests...".to_owned(), Style::default().fg(p.muted));
        f.render_widget(msg.block(block), area);
        return;
    }

    // ── D/E. We have an inbox (render the list, maybe with a warning banner) ──
    let Some(inbox) = &app.inbox else {
        let msg =
            centered_message("Fetching pull requests...".to_owned(), Style::default().fg(p.muted));
        f.render_widget(msg.block(block), area);
        return;
    };

    let Some(active_tab) = app.tabs.active_tab() else {
        f.render_widget(block, area);
        return;
    };
    let active_repo = active_tab.repo.clone();

    let view_mode = app.session.view_mode(&active_repo);
    let viewer_login = inbox.viewer_login.clone();
    let ascii = app.config.show_ascii_glyphs;
    let width = area.width;

    let inner = block.inner(area);

    // ── D. Fetch failed but we have cached data — render banner ───────────────
    let banner_height: u16 = u16::from(app.last_fetch_error.is_some());

    // Render the outer block first, then draw over its interior.
    f.render_widget(block, area);

    // Render the stale-data warning banner if applicable.
    if let Some(err) = &app.last_fetch_error {
        let sync_ago =
            app.inbox_loaded_at.as_ref().map_or_else(|| "unknown".to_string(), humanize_delta);
        let banner_text = format!(" Refresh failed: {err}. Last sync: {sync_ago}. [r] retry");
        let banner_area = Rect::new(inner.x, inner.y, inner.width, 1);
        f.render_widget(
            Paragraph::new(Span::styled(
                truncate(&banner_text, inner.width as usize),
                Style::default().fg(p.warning),
            )),
            banner_area,
        );
    }

    let list_area = Rect::new(
        inner.x,
        inner.y + banner_height,
        inner.width,
        inner.height.saturating_sub(banner_height),
    );

    if list_area.height == 0 {
        return;
    }

    // ── Header line ───────────────────────────────────────────────────────────
    let (view_label, count_label) = match view_mode {
        ViewMode::Prs => {
            let count = inbox.prs.iter().filter(|pr| pr.repo == active_repo).count();
            ("PRs", count)
        }
        ViewMode::Issues => {
            let count = inbox.issues.iter().filter(|i| i.repo == active_repo).count();
            ("issues", count)
        }
    };
    let sync_str = app
        .inbox_loaded_at
        .as_ref()
        .map_or_else(String::new, |t| format!("  last synced: {}", humanize_delta(t)));

    let header_text = format!(" {active_repo} — {count_label} open {view_label}{sync_str} ");
    let header_area = Rect::new(list_area.x, list_area.y, list_area.width, 1);
    f.render_widget(
        Paragraph::new(Span::styled(
            truncate(&header_text, list_area.width as usize),
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        )),
        header_area,
    );

    let table_area = Rect::new(
        list_area.x,
        list_area.y + 1,
        list_area.width,
        list_area.height.saturating_sub(1),
    );

    if table_area.height == 0 {
        return;
    }

    match view_mode {
        ViewMode::Prs => {
            draw_pr_list(f, app, inbox, &active_repo, &viewer_login, ascii, width, table_area);
        }
        ViewMode::Issues => {
            draw_issue_list(f, app, inbox, &active_repo, ascii, width, table_area);
        }
    }
}

/// Render the PR list table.
#[allow(clippy::too_many_arguments)]
fn draw_pr_list(
    f: &mut Frame,
    app: &App,
    inbox: &crate::github::types::Inbox,
    repo: &str,
    viewer_login: &str,
    ascii: bool,
    width: u16,
    area: Rect,
) {
    let p = &app.palette;

    let prs = crate::github::types::sorted_prs_for_repo(inbox, repo);

    if prs.is_empty() {
        let check_ch = if ascii { '+' } else { glyphs::CI_SUCCESS };
        let msg = centered_message(
            format!("{check_ch} No open pull requests"),
            Style::default().fg(p.success),
        );
        f.render_widget(msg, area);
        return;
    }

    // Clamp the selection to the current list length. The stored index can go
    // stale on two paths: (1) a refresh shrinks the list, (2) the user toggled
    // between Prs and Issues (selection is shared per-repo but counts differ).
    let stored = app.selection.get(repo).copied().unwrap_or(0);
    let selected = stored.min(prs.len() - 1);
    let mut table_state = TableState::default().with_selected(Some(selected));

    let cols = pr_columns(width);
    let rows: Vec<Row> = prs.iter().map(|pr| pr_row(pr, viewer_login, width, ascii, p)).collect();

    let selected_style =
        Style::default().bg(p.selection_bg).fg(p.selection_fg).add_modifier(Modifier::BOLD);

    let table = Table::new(rows, cols)
        .style(Style::default().fg(p.foreground).bg(p.background))
        .row_highlight_style(selected_style)
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Never);

    f.render_stateful_widget(table, area, &mut table_state);
}

/// Render the Issue list table.
fn draw_issue_list(
    f: &mut Frame,
    app: &App,
    inbox: &crate::github::types::Inbox,
    repo: &str,
    _ascii: bool,
    width: u16,
    area: Rect,
) {
    let p = &app.palette;

    let issues = crate::github::types::sorted_issues_for_repo(inbox, repo);

    if issues.is_empty() {
        let msg = centered_message("No open issues".to_owned(), Style::default().fg(p.muted));
        f.render_widget(msg, area);
        return;
    }

    // Clamp the stored selection — see note in `draw_pr_list` for the two
    // paths that can leave it stale.
    let stored = app.selection.get(repo).copied().unwrap_or(0);
    let selected = stored.min(issues.len() - 1);
    let mut table_state = TableState::default().with_selected(Some(selected));

    let cols = issue_columns(width);
    let rows: Vec<Row> = issues.iter().map(|issue| issue_row(issue, width, p)).collect();

    let selected_style =
        Style::default().bg(p.selection_bg).fg(p.selection_fg).add_modifier(Modifier::BOLD);

    let table = Table::new(rows, cols)
        .style(Style::default().fg(p.foreground).bg(p.background))
        .row_highlight_style(selected_style)
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Never);

    f.render_stateful_widget(table, area, &mut table_state);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_columns_150_has_six_cols() {
        assert_eq!(pr_columns(150).len(), 6);
    }

    #[test]
    fn pr_columns_110_has_five_cols() {
        assert_eq!(pr_columns(110).len(), 5);
    }

    #[test]
    fn pr_columns_90_has_four_cols() {
        assert_eq!(pr_columns(90).len(), 4);
    }

    #[test]
    fn pr_columns_70_has_three_cols() {
        assert_eq!(pr_columns(70).len(), 3);
    }

    #[test]
    fn truncate_respects_limit() {
        let s = "hello world this is a long title that should be truncated";
        let t = truncate(s, 20);
        assert!(t.chars().count() <= 20, "truncated string too long: {t}");
    }

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("short", 20), "short");
    }
}
