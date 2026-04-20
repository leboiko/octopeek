//! File listing and diff renderers for the Files section.

use std::cell::RefCell;
use std::collections::HashSet;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::github::detail::{FileChangeKind, PrDetail, ReviewThread};
use crate::theme::Palette;
use crate::ui::diff::{DiffFile, DiffLineKind};
use crate::ui::util::truncate;

use super::ThreadIndex;
use super::thread_card::render_thread_card;

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
///
/// # Arguments
///
/// * `thread_index` - Optional index for per-line thread lookups; used in both
///   the overview badges and the diff-view inline expansion.
/// * `expanded` - Set of `(path, lineno)` anchors currently expanded by the
///   user (toggled with `t`). Only consulted in diff mode.
/// * `diff_cursor` - Written by the renderer to track which thread anchor is
///   at or just before the current scroll position, enabling the `t` key to
///   know what to toggle.
/// * `ascii` - Use ASCII glyphs instead of Unicode.
// build_files coordinates two orthogonal features (overview vs diff) with
// several optional inputs; a config struct would be cleaner but would ripple
// through callers for minor ergonomic gain.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_files(
    detail: &PrDetail,
    files_cursor: usize,
    show_diff: bool,
    thread_index: Option<&ThreadIndex>,
    expanded: &HashSet<(String, u32)>,
    diff_cursor: &RefCell<Option<(String, u32)>>,
    p: &Palette,
    ascii: bool,
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
        return build_files_overview(detail, files_cursor, thread_index, p);
    }

    build_files_diff(detail, files_cursor, thread_index, expanded, diff_cursor, p, ascii)
}

/// Files overview: one row per file sorted by magnitude descending.
pub(super) fn build_files_overview(
    detail: &PrDetail,
    files_cursor: usize,
    thread_index: Option<&super::ThreadIndex>,
    p: &Palette,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    // Sort by magnitude descending — same order as the sidebar files list.
    let mut sorted: Vec<&crate::github::detail::FileChange> = detail.files.iter().collect();
    sorted.sort_by_key(|f| std::cmp::Reverse(f.additions + f.deletions));

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

        let mut spans = vec![
            Span::styled(format!("{glyph} "), Style::default().fg(glyph_color)),
            Span::styled(file.path.clone(), row_bg_style.fg(p.foreground)),
            Span::styled("  ".to_owned(), row_bg_style),
            Span::styled(format!("+{}", file.additions), row_bg_style.fg(p.git_new)),
            Span::styled(" ".to_owned(), row_bg_style),
            Span::styled(format!("\u{2212}{}", file.deletions), row_bg_style.fg(p.danger)),
        ];

        // Thread-count badge: `⚑ N` in `palette.warning` when the file has
        // any unresolved (non-outdated) thread, `✓ N` in `palette.muted`
        // when every thread on that file is resolved or outdated. Omitted
        // entirely when the file has no threads at all. Renders after the
        // `+add/-del` stats so the stats column stays aligned across rows.
        if let Some(idx) = thread_index {
            let total = idx.total_for(&file.path);
            if total > 0 {
                let unresolved = idx.unresolved_for(&file.path);
                let (glyph, fg) = if unresolved > 0 {
                    ("\u{2691}", p.warning) // ⚑
                } else {
                    ("\u{2713}", p.muted) // ✓
                };
                spans.push(Span::styled("  ".to_owned(), row_bg_style));
                spans.push(Span::styled(format!("{glyph} {total}"), row_bg_style.fg(fg)));
            }
        }

        lines.push(Line::from(spans));
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
///
/// When `thread_index` is `Some`, calls [`render_diff_with_threads`] so
/// review threads are spliced inline at their anchor lines. When `None`,
/// falls back to plain [`crate::ui::diff::render_diff`] (0.1.7 behaviour).
// The signature grows with the thread-expansion feature; the allow keeps
// clippy happy without sacrificing clarity.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_files_diff(
    detail: &PrDetail,
    files_cursor: usize,
    thread_index: Option<&ThreadIndex>,
    expanded: &HashSet<(String, u32)>,
    diff_cursor: &RefCell<Option<(String, u32)>>,
    p: &Palette,
    ascii: bool,
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
        "J / K: next / previous file   \u{00B7}   j / k: scroll diff   \u{00B7}   t: expand thread"
    } else {
        "j / k: scroll diff   \u{00B7}   t: expand thread"
    };
    let hint_line = Line::from(Span::styled(hint.to_owned(), Style::default().fg(p.dim)));

    let mut lines = vec![header, hint_line, Line::from("")];

    // Body: either the parsed+rendered diff (with optional thread splicing),
    // or a placeholder for binary / too-large / unavailable patches.
    match &file.patch {
        Some(patch) => {
            let diff_file = crate::ui::diff::parse_unified_diff(patch);
            let body = if let Some(index) = thread_index {
                render_diff_with_threads(
                    &diff_file,
                    index,
                    expanded,
                    diff_cursor,
                    &file.path,
                    &detail.review_threads,
                    p,
                    ascii,
                )
            } else {
                crate::ui::diff::render_diff(&diff_file, p)
            };
            lines.extend(body);
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

/// Render a [`DiffFile`] with inline review threads spliced at their anchor lines.
///
/// Walks `file.hunks → DiffHunk.lines` directly, using each `DiffLine.new_lineno`
/// to look up active threads from `index`. After all hunks, if there are overflow
/// threads (file-level or outdated), renders a divider and collapsed cards.
///
/// The `diff_cursor` `RefCell` is written as we go: we record the last
/// thread-anchor line at or before the current scroll position so the `t` key
/// handler can look it up without a second pass.
///
/// # Arguments
///
/// * `file` - The parsed unified diff.
/// * `index` - Pre-built thread index for `(path, lineno)` → thread slice lookups.
/// * `expanded` - Which `(path, lineno)` anchors are currently expanded.
/// * `diff_cursor` - Output cell; updated to the last thread anchor seen so far.
/// * `file_path` - The repository-relative path of the current file.
/// * `all_threads` - The full thread list for the PR (used to resolve indices from `index`).
/// * `palette` - Active colour palette.
/// * `ascii` - Use ASCII glyphs instead of Unicode box-drawing.
// Six data inputs + two style inputs; a config struct would help but the
// tradeoff is noted here rather than imposed on every call site.
#[allow(clippy::too_many_arguments)]
fn render_diff_with_threads(
    file: &DiffFile,
    index: &ThreadIndex,
    expanded: &HashSet<(String, u32)>,
    diff_cursor: &RefCell<Option<(String, u32)>>,
    file_path: &str,
    all_threads: &[ReviewThread],
    palette: &Palette,
    ascii: bool,
) -> Vec<Line<'static>> {
    use crate::ui::diff::render_diff_line;

    if file.hunks.is_empty() {
        return vec![Line::from(Span::styled(
            "(no changes to show)".to_owned(),
            Style::default().fg(palette.dim),
        ))];
    }

    let mut output: Vec<Line<'static>> = Vec::new();

    // Reset the cursor: each render frame re-derives it from scratch.
    *diff_cursor.borrow_mut() = None;

    for (hunk_idx, hunk) in file.hunks.iter().enumerate() {
        // Blank separator between consecutive hunks for readability (mirrors
        // the logic in `render_diff`).
        if hunk_idx > 0 {
            output.push(Line::default());
        }

        // Hunk header — same format as `render_diff`.
        let header_coords = format!(
            "@@ -{},{} +{},{} @@",
            hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
        );
        let mut header_spans = vec![Span::styled(
            header_coords,
            ratatui::style::Style::default()
                .fg(palette.accent)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )];
        if !hunk.section.is_empty() {
            header_spans.push(Span::raw(" "));
            header_spans.push(Span::styled(
                hunk.section.clone(),
                ratatui::style::Style::default().fg(palette.dim),
            ));
        }
        output.push(Line::from(header_spans));

        // Diff lines, with optional thread cards spliced after anchor lines.
        for diff_line in &hunk.lines {
            output.push(render_diff_line(diff_line, palette));

            // Thread cards are only placed after lines that appear in the new
            // file (added or context lines with a `new_lineno`). The
            // `NoNewline` pseudo-line and removed lines have no new-file
            // coordinate and never carry threads.
            if diff_line.kind == DiffLineKind::NoNewline {
                continue;
            }
            let Some(lineno) = diff_line.new_lineno else {
                continue;
            };

            let thread_indices = index.active_at(file_path, lineno);
            if thread_indices.is_empty() {
                continue;
            }

            // Collect the actual thread references (bounds-checked).
            let threads: Vec<&ReviewThread> =
                thread_indices.iter().filter_map(|&i| all_threads.get(i)).collect();
            if threads.is_empty() {
                continue;
            }

            // Update the diff cursor to this anchor. The last anchor at or
            // before the viewport top is what the `t` handler uses — since
            // we walk top-to-bottom, overwriting is correct (last write wins).
            *diff_cursor.borrow_mut() = Some((file_path.to_owned(), lineno));

            let is_expanded = expanded.contains(&(file_path.to_owned(), lineno));
            let card = render_thread_card(&threads, is_expanded, palette, ascii);
            output.extend(card);
        }
    }

    // Overflow block: file-level and outdated threads rendered after all hunks.
    let overflow_indices = index.overflow(file_path);
    if !overflow_indices.is_empty() {
        let overflow_threads: Vec<&ReviewThread> =
            overflow_indices.iter().filter_map(|&i| all_threads.get(i)).collect();

        if !overflow_threads.is_empty() {
            output.push(Line::default()); // blank line before divider

            // Divider line: `╌╌ File-level & outdated threads (N) ╌╌`
            let rule = if ascii { '-' } else { '\u{254C}' }; // ╌
            let label = format!(" File-level & outdated threads ({}) ", overflow_threads.len());
            let rule_str: String = std::iter::repeat_n(rule, 4).collect();
            output.push(Line::from(vec![
                Span::styled(rule_str.clone(), Style::default().fg(palette.dim)),
                Span::styled(label, Style::default().fg(palette.dim)),
                Span::styled(rule_str, Style::default().fg(palette.dim)),
            ]));

            // Each overflow thread renders as a collapsed card (the expand
            // gesture does not apply to overflow threads in 0.1.8).
            for thread in &overflow_threads {
                let card = render_thread_card(
                    std::slice::from_ref(thread),
                    false, // always collapsed in overflow block
                    palette,
                    ascii,
                );
                output.extend(card);
            }
        }
    }

    output
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
    thread_index: Option<&super::ThreadIndex>,
    p: &Palette,
) -> Vec<Line<'static>> {
    let mut sorted_files: Vec<&crate::github::detail::FileChange> = detail.files.iter().collect();
    sorted_files.sort_by_key(|f| std::cmp::Reverse(f.additions + f.deletions));

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

        // Thread badge (sidebar variant): omit the count to save columns,
        // show just `⚑` in warning when any thread is unresolved or `✓`
        // when all are resolved/outdated. Budget the path accordingly so
        // long paths still truncate cleanly.
        let thread_badge: Option<(&'static str, ratatui::style::Color)> =
            thread_index.and_then(|idx| {
                let total = idx.total_for(&file.path);
                if total == 0 {
                    None
                } else if idx.unresolved_for(&file.path) > 0 {
                    Some(("\u{2691}", p.warning))
                } else {
                    Some(("\u{2713}", p.muted))
                }
            });
        let badge_cols = if thread_badge.is_some() { 2 } else { 0 }; // " ⚑"
        let path_budget = sidebar_inner_width.saturating_sub(2).saturating_sub(badge_cols);
        let path = truncate(&file.path, path_budget);

        let is_active_file = selected_is_files && idx == files_cursor;
        let line_style = if is_active_file {
            Style::default().bg(p.selection_bg).fg(p.foreground)
        } else {
            Style::default()
        };

        let mut spans = vec![
            Span::styled(format!("{glyph} "), Style::default().fg(glyph_color)),
            Span::styled(path, line_style.fg(p.foreground)),
        ];
        if let Some((glyph, fg)) = thread_badge {
            spans.push(Span::styled(format!(" {glyph}"), line_style.fg(fg)));
        }
        lines.push(Line::from(spans));
    }

    lines
}
