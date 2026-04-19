//! PR detail panel — renders all sections for a single pull request.
//!
//! Layout:
//! ┌─────────── sticky header (unchanged) ──────────┐
//! ├─── sidebar (28 cols) ───┬── right pane ────────┤
//! │ SECTIONS                │                      │
//! │ ▶ Description           │  content for the     │
//! │   Checks                │  currently selected  │
//! │   Reviews               │  section             │
//! │   Files                 │                      │
//! │   Comments              │                      │
//! ├─────────────────────────┤                      │
//! │ FILES CHANGED           │                      │
//! │   src/a.rs              │                      │
//! └─────────────────────────┴──────────────────────┘
//!
//! ## Thread hierarchy contract
//!
//! `comments_lines` renders review threads with a vertical `│` gutter so the
//! reader can see at a glance that all comments belong to one conversation.
//! The first comment in a thread is the opener; subsequent comments are prefixed
//! with `↳ ` in `palette.dim` to signal "this is a reply".

mod checks;
mod comments;
mod files;
mod header;
mod reviews;
mod sections;

#[cfg(test)]
pub mod tests;

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
};

use crate::app::App;
use crate::github::detail::PrDetail;
use crate::ui::util::render_detail_header;

pub use header::build_header;
pub use sections::build_section;

// ── Section enum ──────────────────────────────────────────────────────────────

/// The five switchable sections in the PR detail sidebar.
///
/// `ALL` gives a stable iteration order; `label()` returns the display string
/// shown in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DetailSection {
    /// The PR description (rendered Markdown body).
    #[default]
    Description,
    /// CI check-run results.
    Checks,
    /// Review approvals / change-requests.
    Reviews,
    /// Files changed listing.
    Files,
    /// Review threads and issue comments.
    Comments,
}

impl DetailSection {
    /// All sections in display order.
    pub const ALL: [DetailSection; 5] = [
        DetailSection::Description,
        DetailSection::Checks,
        DetailSection::Reviews,
        DetailSection::Files,
        DetailSection::Comments,
    ];

    /// Human-readable label used in the sidebar list and help text.
    pub fn label(self) -> &'static str {
        match self {
            DetailSection::Description => "Description",
            DetailSection::Checks => "Checks",
            DetailSection::Reviews => "Reviews",
            DetailSection::Files => "Files",
            DetailSection::Comments => "Comments",
        }
    }

    /// Returns `true` when this section has displayable content in `detail`.
    pub fn has_content(self, detail: &PrDetail) -> bool {
        match self {
            DetailSection::Description => true, // always shown (even if body is empty)
            DetailSection::Checks => !detail.check_runs.is_empty(),
            DetailSection::Reviews => !detail.reviews.is_empty(),
            DetailSection::Files => !detail.files.is_empty(),
            DetailSection::Comments => {
                !detail.review_threads.is_empty() || !detail.issue_comments.is_empty()
            }
        }
    }
}

// ── draw (public entry point) ─────────────────────────────────────────────────

/// Render the PR detail panel into `area`.
///
/// Handles three states:
/// - Fetching (no detail yet): centered spinner text.
/// - Error (fetch failed): error panel with retry hint.
/// - Loaded: full sidebar + right-pane layout beneath a sticky header.
#[allow(clippy::too_many_lines)]
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let p = &app.palette;

    // ── A. Loading ─────────────────────────────────────────────────────────────
    if app.detail_fetching && app.pr_detail.is_none() {
        let widget = Paragraph::new(Line::from(Span::styled(
            "Fetching pull request\u{2026}",
            Style::default().fg(p.dim),
        )))
        .block(Block::default().style(Style::default().bg(p.background)))
        .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(widget, area);
        return;
    }

    // ── B. Error (no cached detail) ────────────────────────────────────────────
    if let Some(err) = &app.detail_error
        && app.pr_detail.is_none()
    {
        let lines = vec![
            Line::from(Span::styled(format!("\u{2716} {err}"), Style::default().fg(p.danger))),
            Line::from(""),
            Line::from(Span::styled(
                "Press Esc to go back, r to retry",
                Style::default().fg(p.dim),
            )),
        ];
        let widget = Paragraph::new(lines)
            .block(Block::default().style(Style::default().bg(p.background)))
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(widget, area);
        return;
    }

    // ── C. Full detail ─────────────────────────────────────────────────────────
    let Some(detail) = &app.pr_detail else {
        return;
    };

    // ── C1. Sticky header (unchanged) ─────────────────────────────────────────
    let header_lines = build_header(detail, p);
    #[allow(clippy::cast_possible_truncation)]
    let header_rows = (header_lines.len() + 2) as u16; // +2 = top pad + bottom rule
    let header_rows = header_rows.min(area.height);
    let vsplits =
        ratatui::layout::Layout::vertical([Constraint::Length(header_rows), Constraint::Min(1)])
            .split(area);
    let header_area = vsplits[0];
    let body_area = vsplits[1];

    render_detail_header(f, header_lines, header_area, p);

    // ── C2. Sidebar + right pane ───────────────────────────────────────────────
    let (sidebar_area, right_area) = if app.sidebar_hidden {
        let dummy = ratatui::layout::Rect { width: 0, ..body_area };
        (dummy, body_area)
    } else {
        let hsplits = ratatui::layout::Layout::horizontal([
            Constraint::Length(app.sidebar_width),
            Constraint::Min(20),
        ])
        .split(body_area);
        (hsplits[0], hsplits[1])
    };

    // Sidebar sub-split: sections list (top) + files list (bottom).
    let sidebar_sections_height: u16 = 7;
    let vsidebar = ratatui::layout::Layout::vertical([
        Constraint::Length(sidebar_sections_height.min(sidebar_area.height)),
        Constraint::Min(0),
    ])
    .split(sidebar_area);
    let sections_area = vsidebar[0];
    let files_area = vsidebar[1];

    app.pr_detail_sidebar_rects.set((sections_area, files_area));

    // ── C3. Render sections list ───────────────────────────────────────────────
    let selected_section = app.pr_detail_selected_section;

    if !app.sidebar_hidden {
        let indicator = if app.config.show_ascii_glyphs { "> " } else { "\u{25b6} " }; // ▶
        let placeholder = "  ";

        let mut section_lines: Vec<Line<'static>> = Vec::new();
        section_lines.push(Line::from(Span::styled(
            "SECTIONS".to_owned(),
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        )));
        for sec in DetailSection::ALL {
            let is_selected = sec == selected_section;
            let prefix = if is_selected { indicator } else { placeholder };
            let style = if is_selected {
                Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
            } else if sec.has_content(detail) {
                Style::default().fg(p.foreground)
            } else {
                Style::default().fg(p.dim)
            };
            section_lines.push(Line::from(Span::styled(format!("{prefix}{}", sec.label()), style)));
        }

        let sections_block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(p.border_focused))
            .style(Style::default().bg(p.background))
            .padding(Padding::new(1, 0, 0, 0));
        let sections_inner = sections_block.inner(sections_area);
        f.render_widget(Paragraph::new(section_lines).block(sections_block), sections_area);

        // ── C4. Render files list ──────────────────────────────────────────────
        let mut file_list_lines: Vec<Line<'static>> = Vec::new();
        let files_header = format!("FILES CHANGED ({})", detail.changed_files_count);
        file_list_lines.push(Line::from(Span::styled(
            files_header,
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        )));

        let sidebar_inner_width = usize::from(sections_inner.width).saturating_sub(1);
        let files_cursor = app.pr_detail_files_cursor;
        let selected_is_files = selected_section == DetailSection::Files;

        file_list_lines.extend(files::sidebar_file_lines(
            detail,
            files_cursor,
            selected_is_files,
            sidebar_inner_width,
            p,
        ));

        let files_block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(p.border_focused))
            .style(Style::default().bg(p.background))
            .padding(Padding::new(1, 0, 0, 0));
        let files_scroll = app.pr_detail_sidebar_scroll;
        f.render_widget(
            Paragraph::new(file_list_lines).block(files_block).scroll((files_scroll, 0)),
            files_area,
        );

        let _ = sections_inner;
    }

    // ── C5. Render right pane ──────────────────────────────────────────────────
    let (content_lines, alt_bg_ranges) = build_section(
        selected_section,
        detail,
        app.pr_detail_files_cursor,
        app.pr_detail_files_show_diff,
        app.detail_comments_expanded,
        p,
        app.config.show_ascii_glyphs,
    );

    let left_padding = if app.sidebar_hidden { 3 } else { 2 };
    let block = Block::default()
        .style(Style::default().bg(p.background).fg(p.foreground))
        .padding(Padding::new(left_padding, 2, 0, 0));
    let inner = block.inner(right_area);

    app.pr_detail_viewport.set(inner);
    app.pr_detail_right_viewport.set(inner);

    let scroll = app.right_pane_scroll();

    let tinted_lines = header::apply_alt_bg(
        &content_lines,
        &alt_bg_ranges,
        p.help_bg,
        inner.width,
        !app.copy_mode.active,
    );

    if app.sidebar_hidden && inner.height > 0 {
        let hint_area = ratatui::layout::Rect { x: right_area.x, y: inner.y, width: 1, height: 1 };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "\u{203a}", // ›
                Style::default().fg(p.dim),
            ))),
            hint_area,
        );
    }

    let widget = if app.copy_mode.active {
        let overlaid = crate::ui::copy_mode::apply_overlay(&tinted_lines, &app.copy_mode, p);
        Paragraph::new(overlaid)
            .block(block)
            .style(Style::default().bg(p.background).fg(p.foreground))
            .scroll((scroll, app.copy_mode.h_scroll))
    } else {
        Paragraph::new(tinted_lines)
            .block(block)
            .style(Style::default().bg(p.background).fg(p.foreground))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
    };

    f.render_widget(widget, right_area);
}
