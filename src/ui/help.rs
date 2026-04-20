//! Full-screen help overlay listing all keyboard shortcuts.
//!
//! Closed with `?`, `q`, or `Esc`.

use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Render the centered help overlay.
#[allow(clippy::too_many_lines)]
pub fn draw(f: &mut Frame, app: &App) {
    let p = &app.palette;

    let header_style = Style::default().fg(p.accent).add_modifier(Modifier::BOLD);
    let key_style = Style::default().fg(p.accent_alt).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(p.foreground);
    let dim_style = p.dim_style();

    let lines: Vec<Line> = vec![
        Line::from(Span::styled("octopeek Keyboard Shortcuts", header_style)),
        Line::from(""),
        Line::from(Span::styled("── General ──────────────────────────────────", dim_style)),
        shortcut("q", "Quit", key_style, desc_style),
        shortcut("?", "Toggle this help overlay", key_style, desc_style),
        shortcut(
            "Tab / Shift+Tab",
            "Next / previous repo tab (on dashboard)",
            key_style,
            desc_style,
        ),
        shortcut("[ / ]", "Previous / next repo tab (works in detail)", key_style, desc_style),
        shortcut("1 – 9", "Jump to repo tab N (on dashboard)", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled("── Dashboard (Phase 2+) ─────────────────────", dim_style)),
        shortcut("j / Down", "Move cursor down", key_style, desc_style),
        shortcut("k / Up", "Move cursor up", key_style, desc_style),
        shortcut("g", "Jump to top of list", key_style, desc_style),
        shortcut("G", "Jump to bottom of list", key_style, desc_style),
        shortcut("Enter", "Open PR / issue detail", key_style, desc_style),
        shortcut("Esc", "Return to dashboard from detail", key_style, desc_style),
        shortcut("i", "Toggle between PR and Issue view", key_style, desc_style),
        shortcut("r", "Refresh current tab", key_style, desc_style),
        shortcut("R", "Refresh all tabs", key_style, desc_style),
        shortcut("A", "Toggle all-repos / mine-only view", key_style, desc_style),
        shortcut("n / N", "Next / previous match", key_style, desc_style),
        shortcut("f", "Filter / find", key_style, desc_style),
        shortcut("b", "Back", key_style, desc_style),
        shortcut("c", "Open theme picker", key_style, desc_style),
        shortcut("p", "Open repo picker", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled("── First-run setup ──────────────────────────", dim_style)),
        Line::from(Span::styled(
            "  A welcome wizard auto-opens on first launch (empty config)",
            Style::default().fg(p.dim),
        )),
        Line::from(Span::styled(
            "  and lists repos you're active in. Use Space + Enter there,",
            Style::default().fg(p.dim),
        )),
        Line::from(Span::styled(
            "  or press `p` any time to add / remove repos by hand.",
            Style::default().fg(p.dim),
        )),
        Line::from(""),
        Line::from(Span::styled("── PR Detail Sections ───────────────────────", dim_style)),
        shortcut("! (Shift+1)", "Description section", key_style, desc_style),
        shortcut("@ (Shift+2)", "Checks section", key_style, desc_style),
        shortcut("# (Shift+3)", "Reviews section", key_style, desc_style),
        shortcut("$ (Shift+4)", "Files section", key_style, desc_style),
        shortcut("% (Shift+5)", "Comments section", key_style, desc_style),
        shortcut("z", "Hide / show outdated review threads", key_style, desc_style),
        shortcut("F (Shift+f)", "Jump to Files section", key_style, desc_style),
        shortcut("J / K (in Files)", "Next / previous file in diff view", key_style, desc_style),
        shortcut(
            "t (in Files diff)",
            "Expand/collapse the review thread at the cursor line",
            key_style,
            desc_style,
        ),
        shortcut(
            "T (in Files diff)",
            "Collapse every expanded thread in the open file",
            key_style,
            desc_style,
        ),
        shortcut(
            "j / k / d / u / g / G",
            "Scroll right pane (per-section / per-file)",
            key_style,
            desc_style,
        ),
        Line::from(""),
        Line::from(Span::styled("── PR / Issue Actions (Phase 4+) ────────────", dim_style)),
        shortcut("o", "Open in browser", key_style, desc_style),
        shortcut("y", "Copy URL to clipboard", key_style, desc_style),
        shortcut("c", "Checkout PR branch (with confirmation)", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled("── Copy Mode (press v in detail) ────────────", dim_style)),
        shortcut("v", "Enter copy mode", key_style, desc_style),
        shortcut("h / j / k / l", "Move cursor (or arrows)", key_style, desc_style),
        shortcut("0 / $", "Cursor to line start / end", key_style, desc_style),
        shortcut("g / G", "Cursor to top / bottom", key_style, desc_style),
        shortcut("V", "Toggle selection anchor", key_style, desc_style),
        shortcut("y", "Yank selection to clipboard", key_style, desc_style),
        shortcut("Y", "Yank entire current line", key_style, desc_style),
        shortcut("Esc", "Exit copy mode", key_style, desc_style),
        shortcut("Mouse wheel", "Scroll detail by 3 lines", key_style, desc_style),
        shortcut("Mouse click/drag", "Place cursor / select range", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled("── Inbox Roles (Phase 3) ────────────────────", dim_style)),
        Line::from(Span::styled(
            "  A = Author   R = Review requested   @ = Assignee",
            Style::default().fg(p.dim),
        )),
        Line::from(""),
        Line::from(Span::styled("Press ?, q, or Esc to close", Style::default().fg(p.dim))),
    ];

    let height = crate::cast::u16_sat(lines.len()) + 2;
    let width = 58u16;

    let area = centered_rect(width, height, f.area());

    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.border_focused))
        .style(Style::default().bg(p.help_bg));

    let paragraph = Paragraph::new(lines).block(block);

    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}

/// Build a two-column shortcut line: `  {key:<22} {desc}`.
fn shortcut<'a>(key: &'a str, desc: &'a str, key_style: Style, desc_style: Style) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {key:<22}"), key_style),
        Span::styled(desc, desc_style),
    ])
}

/// Compute a centered [`Rect`] of the requested dimensions within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).split(area);
    Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center).split(vertical[0])[0]
}
