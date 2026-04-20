//! Collapsed / expanded inline thread-card renderer for the diff view.
//!
//! A thread card is inserted immediately after a diff line that has one or
//! more review threads anchored to it. In the collapsed state it shows a
//! single summary line; pressing `t` toggles it to the expanded state, which
//! renders the full thread body (header, optional diff-hunk excerpt, and
//! comment bodies with gutter rails).

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::github::detail::ReviewThread;
use crate::theme::Palette;

use super::comments::{render_thread_body, thread_gutter};

// Collapsed card left pad: 13 spaces so the summary text aligns with the
// content column of the diff (5 old-lineno + 1 space + 5 new-lineno + 1 space
// + 1 prefix-char + 1 space = 14 columns for the diff, but we want to sit
// just inside that gutter boundary).
const CARD_PAD: &str = "             "; // 13 spaces

/// Render one or more review threads anchored to a single diff line as a
/// sequence of styled `Line`s.
///
/// # Arguments
///
/// * `threads` - The threads anchored at this line (non-empty slice).
/// * `expanded` - When `true` the full thread body is rendered; when `false`
///   only a one-line summary is emitted.
/// * `palette` - Active colour palette.
/// * `ascii` - Use ASCII glyphs instead of Unicode box-drawing.
///
/// # Returns
///
/// A non-empty `Vec<Line<'static>>`. In collapsed mode this is always exactly
/// one line. In expanded mode it is at least two lines (header + first body
/// row).
pub(super) fn render_thread_card(
    threads: &[&ReviewThread],
    expanded: bool,
    palette: &Palette,
    ascii: bool,
) -> Vec<Line<'static>> {
    let total = threads.len();
    let unresolved = threads.iter().filter(|t| !t.is_resolved && !t.is_outdated).count();

    if !expanded {
        return vec![collapsed_summary_line(total, unresolved, palette, ascii)];
    }

    // Expanded: render each thread body in full, separated by a blank line.
    let gutter = thread_gutter(ascii);
    let reply_glyph = if ascii { "> " } else { "\u{21b3} " }; // ↳

    let mut out: Vec<Line<'static>> = Vec::new();

    for (idx, thread) in threads.iter().enumerate() {
        if idx > 0 {
            // Blank gutter separator between multiple threads at the same line.
            out.push(Line::from(vec![Span::raw(CARD_PAD)]));
        }

        // Collapse hint in the expanded header line: suffix `[t] collapse  [T]`.
        // We render the thread body first, then replace its header line with
        // an annotated version.
        let mut body = render_thread_body(thread, true, gutter, reply_glyph, palette, ascii);

        // Annotate the first line (thread header) with the keyboard hints.
        if let Some(header_line) = body.first_mut() {
            header_line.spans.push(Span::styled(
                "    [t] collapse  [T] collapse all".to_owned(),
                Style::default().fg(palette.dim),
            ));
        }

        // Left-pad every body line by CARD_PAD so it aligns with the diff
        // content column.
        for line in &mut body {
            line.spans.insert(0, Span::raw(CARD_PAD));
        }

        out.extend(body);
    }

    out
}

/// Build the single collapsed summary line.
///
/// Format (unicode): `             ○ N threads · M unresolved    [t] expand`
/// Format (ascii):   `             ? N threads · M unresolved    [t] expand`
///
/// When all threads are resolved the circle/check glyph changes and the
/// unresolved phrase is omitted.
fn collapsed_summary_line(
    total: usize,
    unresolved: usize,
    palette: &Palette,
    ascii: bool,
) -> Line<'static> {
    // Choose glyph and colour based on resolution state.
    let (glyph, glyph_color) = if unresolved > 0 {
        // Open circle in warning colour — attention required.
        (if ascii { "?" } else { "\u{25CB}" }, palette.warning) // ○
    } else {
        // Check mark in muted colour — fully resolved.
        (if ascii { "+" } else { "\u{2714}" }, palette.muted) // ✔
    };

    let thread_word = if total == 1 { "thread" } else { "threads" };
    let count_text = format!(" {total} {thread_word}");

    // Unresolved suffix: only shown when at least one thread needs attention.
    let unresolved_text = if unresolved > 0 {
        format!(" \u{00B7} {unresolved} unresolved") // · N unresolved
    } else {
        String::new()
    };

    Line::from(vec![
        Span::raw(CARD_PAD),
        Span::styled(glyph, Style::default().fg(glyph_color)),
        Span::styled(count_text, Style::default().fg(palette.foreground)),
        Span::styled(unresolved_text, Style::default().fg(palette.warning)),
        Span::styled(
            "    [t] expand".to_owned(),
            Style::default().fg(palette.dim).add_modifier(Modifier::DIM),
        ),
    ])
}
