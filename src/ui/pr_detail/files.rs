//! File listing and diff renderers for the Files section.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::github::detail::{FileChangeKind, PrDetail};
use crate::theme::Palette;
use crate::ui::util::truncate;

/// Glyph for a file change kind.
pub(super) fn file_kind_glyph(kind: FileChangeKind) -> &'static str {
    match kind {
        FileChangeKind::Added => "\u{271A}",    // ✚
        FileChangeKind::Modified => "\u{270E}", // ✎
        FileChangeKind::Deleted => "\u{2702}",  // ✂
        FileChangeKind::Renamed => "\u{2192}",  // →
        FileChangeKind::Copied | FileChangeKind::Changed => "\u{00B7}", // ·
    }
}

/// Alternating-bg range helper: record `(start_line_idx, end_line_idx_exclusive)`
/// for the lines belonging to a single top-level comment/thread when the
/// current parity calls for a tint.
pub(super) fn push_alt_range(ranges: &mut Vec<(u16, u16)>, start: usize, end: usize, alt_on: bool) {
    if !alt_on || end <= start {
        return;
    }
    let start = u16::try_from(start).unwrap_or(u16::MAX);
    let end = u16::try_from(end).unwrap_or(u16::MAX);
    ranges.push((start, end));
}

/// Build lines for the Files section.
///
/// When `show_diff` is `false` (overview mode), renders one line per file
/// sorted by magnitude with `+add` / `−del` counts and a footer hint.
/// When `show_diff` is `true` (diff mode), renders the unified diff for the
/// file pointed at by `files_cursor` with a header banner and navigation hint.
///
/// Returns `(lines, alt_bg_ranges)`; ranges are always empty here
/// (alt-bg tinting is a comments-only feature).
pub(super) fn build_files(
    detail: &PrDetail,
    files_cursor: usize,
    show_diff: bool,
    p: &Palette,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    if detail.files.is_empty() {
        return (
            vec![Line::from(Span::styled(
                "No files changed".to_owned(),
                Style::default().fg(p.dim),
            ))],
            Vec::new(),
        );
    }

    if !show_diff {
        return build_files_overview(detail, files_cursor, p);
    }

    build_files_diff(detail, files_cursor, p)
}

/// Files overview: one row per file sorted by magnitude descending.
pub(super) fn build_files_overview(
    detail: &PrDetail,
    files_cursor: usize,
    p: &Palette,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    // Sort by magnitude descending — same order as the sidebar files list.
    let mut sorted: Vec<&crate::github::detail::FileChange> = detail.files.iter().collect();
    sorted.sort_by(|a, b| (b.additions + b.deletions).cmp(&(a.additions + a.deletions)));

    let cursor = files_cursor.min(sorted.len().saturating_sub(1));
    let mut lines = Vec::with_capacity(sorted.len() + 1);

    for (idx, file) in sorted.iter().enumerate() {
        let glyph = file_kind_glyph(file.change_kind);
        let glyph_color = match file.change_kind {
            FileChangeKind::Added => p.success,
            FileChangeKind::Modified => p.warning,
            FileChangeKind::Deleted => p.danger,
            FileChangeKind::Renamed => p.accent,
            FileChangeKind::Copied | FileChangeKind::Changed => p.muted,
        };

        let is_cursor = idx == cursor;
        // Selected row gets an inversion highlight so the user can see which
        // file J/K would open when pressing F.
        let row_bg_style = if is_cursor {
            Style::default().bg(p.selection_bg).fg(p.selection_fg)
        } else {
            Style::default()
        };

        let line = Line::from(vec![
            Span::styled(format!("{glyph} "), Style::default().fg(glyph_color)),
            Span::styled(file.path.clone(), row_bg_style.fg(p.foreground)),
            Span::styled("  ".to_owned(), row_bg_style),
            Span::styled(format!("+{}", file.additions), row_bg_style.fg(p.git_new)),
            Span::styled(" ".to_owned(), row_bg_style),
            Span::styled(format!("\u{2212}{}", file.deletions), row_bg_style.fg(p.danger)),
        ]);
        lines.push(line);
    }

    // Footer hint — one line, always visible.
    lines.push(Line::from(Span::styled(
        "$ overview  \u{00B7}  F open diff  \u{00B7}  J/K cycle file  \u{00B7}  click a file to open"
            .to_owned(),
        Style::default().fg(p.dim),
    )));

    (lines, Vec::new())
}

/// Files diff: unified diff for the currently-cursor file, with banner + hint.
///
/// When the pointed file has `patch == None` (binary, too large, REST fetch
/// failed, or beyond the 30-file cap), falls back to a dim placeholder so
/// the user sees something instead of a blank pane.
pub(super) fn build_files_diff(
    detail: &PrDetail,
    files_cursor: usize,
    p: &Palette,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    let idx = files_cursor.min(detail.files.len() - 1);
    let file = &detail.files[idx];
    let total = detail.files.len();

    // File-header banner: cursor position (1-based), file path, and +add/-del
    // stats — tells the reader exactly which file in the list they're on
    // when they cycle with `J`/`K`.
    let header = Line::from(vec![
        Span::styled(format!("[{}/{}] ", idx + 1, total), Style::default().fg(p.dim)),
        Span::styled(
            file.path.clone(),
            Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ".to_owned(), Style::default()),
        Span::styled(format!("+{}", file.additions), Style::default().fg(p.git_new)),
        Span::styled(" ".to_owned(), Style::default()),
        Span::styled(format!("\u{2212}{}", file.deletions), Style::default().fg(p.danger)),
    ]);

    // Navigation hint right under the header — exact keystrokes, not a
    // generic "see help" nudge, so the user doesn't have to leave the view.
    let hint = if total > 1 {
        "J / K: next / previous file   \u{00B7}   j / k: scroll diff"
    } else {
        "j / k: scroll diff"
    };
    let hint_line = Line::from(Span::styled(hint.to_owned(), Style::default().fg(p.dim)));

    let mut lines = vec![header, hint_line, Line::from("")];

    // Body: either the parsed+rendered diff, or a placeholder.
    match &file.patch {
        Some(patch) => {
            let diff = crate::ui::diff::parse_unified_diff(patch);
            lines.extend(crate::ui::diff::render_diff(&diff, p));
        }
        None => {
            lines.push(Line::from(Span::styled(
                "Patch unavailable — binary file, too large, beyond the 30-file cap, or fetch failed.".to_owned(),
                Style::default().fg(p.dim),
            )));
        }
    }

    (lines, Vec::new())
}

/// Build the sidebar file list lines for the files panel.
///
/// Renders one line per file sorted by magnitude descending, with glyph
/// colorisation and cursor highlight. Called from `draw`.
pub(super) fn sidebar_file_lines(
    detail: &crate::github::detail::PrDetail,
    files_cursor: usize,
    selected_is_files: bool,
    sidebar_inner_width: usize,
    p: &Palette,
) -> Vec<Line<'static>> {
    let mut sorted_files: Vec<&crate::github::detail::FileChange> = detail.files.iter().collect();
    sorted_files.sort_by(|a, b| (b.additions + b.deletions).cmp(&(a.additions + a.deletions)));

    let mut lines = Vec::with_capacity(sorted_files.len());

    for (idx, file) in sorted_files.iter().enumerate() {
        let glyph = file_kind_glyph(file.change_kind);
        let glyph_color = match file.change_kind {
            FileChangeKind::Added => p.success,
            FileChangeKind::Modified => p.warning,
            FileChangeKind::Deleted => p.danger,
            FileChangeKind::Renamed => p.accent,
            FileChangeKind::Copied | FileChangeKind::Changed => p.muted,
        };
        let path_budget = sidebar_inner_width.saturating_sub(2); // 2 = glyph + space
        let path = truncate(&file.path, path_budget);

        let is_active_file = selected_is_files && idx == files_cursor;
        let line_style = if is_active_file {
            Style::default().bg(p.selection_bg).fg(p.foreground)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{glyph} "), Style::default().fg(glyph_color)),
            Span::styled(path, line_style.fg(p.foreground)),
        ]));
    }

    lines
}
