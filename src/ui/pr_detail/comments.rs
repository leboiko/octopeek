//! Comment section builder: review threads and issue comments.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::github::detail::PrDetail;
use crate::theme::Palette;
use crate::ui::diff::{parse_unified_diff, render_diff};
use crate::ui::markdown::render_markdown;
use crate::ui::util::humanize_delta;
use crate::ui::util::section_header;

use super::files::push_alt_range;

/// Maximum number of rendered rows (header + diff lines) used when showing
/// the `diff_hunk` excerpt under a thread header. GitHub typically returns
/// 4–12 lines of context in `diffHunk`; this cap protects against a runaway
/// hunk dominating the Comments section on old comments with huge contexts.
const DIFF_HUNK_EXCERPT_MAX_ROWS: usize = 12;

// ── Diff hunk excerpt ─────────────────────────────────────────────────────────

/// Render the `diffHunk` string GitHub ships with each review comment as a
/// small styled code excerpt, indented to visually belong to the thread
/// above it.
///
/// Returns an empty `Vec` when `hunk` is `None`, empty, or fails to parse as
/// a unified diff — the caller simply emits no excerpt in that case and the
/// thread still renders with just the header + comment bodies. Defensive
/// behaviour matters here because older cached `PrDetail` payloads predate
/// the field's addition to our GraphQL query.
fn diff_hunk_excerpt(hunk: Option<&str>, p: &Palette) -> Vec<Line<'static>> {
    let Some(text) = hunk.map(str::trim).filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let parsed = parse_unified_diff(text);
    if parsed.hunks.is_empty() {
        return Vec::new();
    }
    let rendered = render_diff(&parsed, p);

    // Indent by 4 columns so the excerpt sits inside the thread block without
    // competing with the thread's `│` gutter (which starts at column 2).
    let indent = "    ";
    let truncated = rendered.len() > DIFF_HUNK_EXCERPT_MAX_ROWS;
    let visible_rows = rendered.len().min(DIFF_HUNK_EXCERPT_MAX_ROWS);

    let mut out: Vec<Line<'static>> = Vec::with_capacity(visible_rows + usize::from(truncated));
    for mut line in rendered.into_iter().take(visible_rows) {
        line.spans.insert(0, Span::raw(indent));
        out.push(line);
    }
    if truncated {
        out.push(Line::from(Span::styled(
            format!("{indent}\u{2026}  hunk truncated"),
            Style::default().fg(p.dim),
        )));
    }
    out
}

// ── Gutter helpers ────────────────────────────────────────────────────────────

/// The Unicode vertical gutter prepended to every body line inside a review thread.
const THREAD_GUTTER_UNICODE: &str = "  \u{2502}  "; // "  │  "

/// The ASCII fallback used when `config.show_ascii_glyphs` is true; some
/// terminals (older `PuTTY`, ssh through limited charsets) render Unicode box
/// drawing as replacement squares.
const THREAD_GUTTER_ASCII: &str = "  |  ";

/// Return the gutter string appropriate for the current `ascii` setting.
pub(super) fn thread_gutter(ascii: bool) -> &'static str {
    if ascii { THREAD_GUTTER_ASCII } else { THREAD_GUTTER_UNICODE }
}

/// Wrap rendered markdown lines with the thread gutter prefix.
///
/// Each `Line` from `render_markdown` gets a leading gutter span prepended,
/// coloured with `gutter_fg`. The opener uses the default
/// `palette.block_quote_border`; replies use a distinct colour (normally
/// `palette.accent_alt`) so the reply's vertical rail visually separates
/// it from the thread opener sitting right above.
pub(super) fn gutter_lines(
    md_lines: Vec<Line<'static>>,
    gutter_fg: ratatui::style::Color,
    ascii: bool,
) -> Vec<Line<'static>> {
    md_lines
        .into_iter()
        .map(|mut line| {
            let inherited_bg = line.spans.first().and_then(|s| s.style.bg);
            let mut style = Style::default().fg(gutter_fg);
            if let Some(bg) = inherited_bg {
                style = style.bg(bg);
            }
            let gutter_span = Span::styled(thread_gutter(ascii), style);
            line.spans.insert(0, gutter_span);
            line
        })
        .collect()
}

/// Prepend `prefix` (a raw-style indent) to each rendered markdown line.
pub(super) fn indent_lines(
    md_lines: Vec<Line<'static>>,
    prefix: &'static str,
) -> Vec<Line<'static>> {
    md_lines
        .into_iter()
        .map(|mut line| {
            line.spans.insert(0, Span::raw(prefix));
            line
        })
        .collect()
}

// ── Thread header ─────────────────────────────────────────────────────────────

/// Build the single header line for a review thread.
pub(super) fn thread_header_line(
    thread: &crate::github::detail::ReviewThread,
    p: &Palette,
    ascii: bool,
) -> Line<'static> {
    let (glyph, glyph_color, status_text) = if thread.is_outdated {
        (if ascii { "D" } else { "\u{25C6}" }, p.muted, "outdated")
    } else if thread.is_resolved {
        (if ascii { "+" } else { "\u{2713}" }, p.muted, "resolved")
    } else {
        (if ascii { "!" } else { "\u{2691}" }, p.warning, "unresolved")
    };

    let location =
        thread.line.map_or_else(|| thread.path.clone(), |ln| format!("{}:{ln}", thread.path));

    let n = thread.comments.len();
    let count_str = format!("  \u{00B7}  {n} comment{}", if n == 1 { "" } else { "s" });
    let status_str = format!("  \u{00B7}  {status_text}");

    Line::from(vec![
        Span::styled(format!("  {glyph} "), Style::default().fg(glyph_color)),
        Span::styled(location, Style::default().fg(p.accent)),
        Span::styled(count_str, Style::default().fg(p.dim)),
        Span::styled(status_str, Style::default().fg(p.dim)),
    ])
}

// ── Comment section builder ───────────────────────────────────────────────────

/// Build comment section lines, with expansion controlled by `expanded`.
///
/// Returns `(lines, unresolved_thread_relative_offsets, alt_bg_ranges)`.
/// Offsets are relative to the start of the comment block (0 = first line
/// of the block header). Callers add the header's absolute Y to get global anchors.
#[allow(clippy::too_many_lines)]
pub(super) fn comments_lines(
    detail: &PrDetail,
    expanded: bool,
    p: &Palette,
    ascii: bool,
) -> (Vec<Line<'static>>, Vec<u16>, Vec<(u16, u16)>) {
    let gutter = thread_gutter(ascii);
    let reply_glyph = if ascii { "> " } else { "\u{21b3} " };
    let unresolved_count = detail.review_threads.iter().filter(|t| !t.is_resolved).count();
    let total_threads = detail.review_threads.len();
    let total_comments = detail.issue_comments.len();

    let mut lines = Vec::new();
    let mut unresolved_offsets: Vec<u16> = Vec::new();
    let mut alt_bg_ranges: Vec<(u16, u16)> = Vec::new();

    // Sort threads: unresolved first.
    let mut threads: Vec<&crate::github::detail::ReviewThread> =
        detail.review_threads.iter().collect();
    threads.sort_by_key(|t| t.is_resolved);

    // When collapsed, allow up to 10 total items (threads + issue comments).
    let max_items = if expanded { usize::MAX } else { 10 };
    let mut items_shown = 0;
    // Toggle per top-level item (thread or standalone issue comment) so every
    // other conversation gets a subtle bg tint the user can group visually.
    let mut alt_on = false;

    // ── Review threads ────────────────────────────────────────────────────────
    for thread in &threads {
        if items_shown >= max_items {
            break;
        }

        // Record the thread-header offset for unresolved threads so `n`/`N`
        // navigation jumps to the right line.
        if !thread.is_resolved {
            #[allow(clippy::cast_possible_truncation)]
            unresolved_offsets.push(lines.len() as u16);
        }

        let alt_start = lines.len();

        // Thread header: `  ⚑ src/foo.rs:42  ·  2 comments  ·  unresolved`
        lines.push(thread_header_line(thread, p, ascii));

        // Inline code excerpt from GitHub's `diffHunk` (if present). Gives the
        // reader the ±N lines of context the comment was anchored to without
        // forcing a jump into the Files section. See `diff_hunk_excerpt` for
        // the empty-input / parse-failure fallback.
        let hunk_lines = diff_hunk_excerpt(thread.diff_hunk.as_deref(), p);
        if !hunk_lines.is_empty() {
            lines.extend(hunk_lines);
            // Blank separator before the comment bodies so the excerpt reads
            // as a distinct inset block, not part of the first comment.
            lines.push(Line::from(""));
        }

        for (idx, comment) in thread.comments.iter().enumerate() {
            let age = humanize_delta(&comment.created_at);

            let is_reply = idx > 0;
            let gutter_fg = if is_reply { p.accent_alt } else { p.block_quote_border };

            let author_line = if is_reply {
                Line::from(vec![
                    Span::styled(gutter, Style::default().fg(gutter_fg)),
                    Span::styled(
                        reply_glyph,
                        Style::default().fg(p.accent_alt).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("@{}", comment.author),
                        Style::default().fg(p.accent_alt).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("  {age}"), Style::default().fg(p.dim)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(gutter, Style::default().fg(gutter_fg)),
                    Span::styled(
                        format!("@{}", comment.author),
                        Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("  {age}"), Style::default().fg(p.dim)),
                ])
            };
            lines.push(author_line);

            // Render the comment body as markdown, wrapped in the `│` gutter.
            let body = comment.body_markdown.trim();
            let rendered = render_markdown(body, p);
            let total_rendered = rendered.len();

            let (visible_rendered, truncated) = if !expanded && total_rendered > 6 {
                (rendered.into_iter().take(6).collect::<Vec<_>>(), true)
            } else {
                (rendered, false)
            };

            let body_lines = if is_reply {
                gutter_lines(indent_lines(visible_rendered, "  "), gutter_fg, ascii)
            } else {
                gutter_lines(visible_rendered, gutter_fg, ascii)
            };
            lines.extend(body_lines);

            if truncated {
                lines.push(Line::from(vec![
                    Span::styled(gutter, Style::default().fg(gutter_fg)),
                    Span::styled("[m] expand", Style::default().fg(p.dim)),
                ]));
            }

            // Blank gutter line between comments within the same thread.
            if idx + 1 < thread.comments.len() {
                lines.push(Line::from(vec![Span::styled(
                    gutter,
                    Style::default().fg(p.accent_alt),
                )]));
            }
        }

        // Close the alt-bg range BEFORE the trailing blank separator.
        push_alt_range(&mut alt_bg_ranges, alt_start, lines.len(), alt_on);
        alt_on = !alt_on;

        // Blank line between threads (no gutter — clean visual separator).
        lines.push(Line::from(""));
        items_shown += 1;
    }

    // ── Issue comments ────────────────────────────────────────────────────────
    for comment in &detail.issue_comments {
        if items_shown >= max_items {
            break;
        }
        let age = humanize_delta(&comment.created_at);
        let alt_start = lines.len();

        // Author header: `@handle` bold, then `  <age>` dim.
        lines.push(Line::from(vec![
            Span::styled(
                format!("@{}", comment.author),
                Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {age}"), Style::default().fg(p.dim)),
        ]));

        // Body rendered as markdown, indented by two spaces (no gutter).
        let body = comment.body_markdown.trim();
        let rendered = render_markdown(body, p);
        let total_rendered = rendered.len();

        let (visible_rendered, truncated) = if !expanded && total_rendered > 6 {
            (rendered.into_iter().take(6).collect::<Vec<_>>(), true)
        } else {
            (rendered, false)
        };

        lines.extend(indent_lines(visible_rendered, "  "));

        if truncated {
            lines.push(Line::from(Span::styled("  [m] expand", Style::default().fg(p.dim))));
        }

        push_alt_range(&mut alt_bg_ranges, alt_start, lines.len(), alt_on);
        alt_on = !alt_on;

        lines.push(Line::from("")); // blank separator between comments
        items_shown += 1;
    }

    let total_items = total_threads + total_comments;
    if !expanded && total_items > 10 {
        lines.push(Line::from(Span::styled(
            format!("  ... {} more  [m] to expand", total_items - items_shown),
            Style::default().fg(p.dim),
        )));
    }

    // Build the section header (including counts) as a prefix.
    let header = section_header(
        &format!("COMMENTS ({total_threads} threads \u{00B7} {unresolved_count} unresolved)"),
        p,
    );

    let mut all_lines = vec![header];
    all_lines.extend(lines);

    // Shift unresolved offsets by 1 to account for the header line we prepended.
    let shifted_offsets = unresolved_offsets.iter().map(|&o| o + 1).collect();
    let shifted_alt_ranges: Vec<(u16, u16)> =
        alt_bg_ranges.into_iter().map(|(a, b)| (a + 1, b + 1)).collect();

    (all_lines, shifted_offsets, shifted_alt_ranges)
}

/// Build lines for the Comments section.
///
/// Returns `(lines, alt_bg_ranges)`.
pub(super) fn build_comments(
    detail: &PrDetail,
    comments_expanded: bool,
    p: &Palette,
    ascii: bool,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    let has_comments = !detail.review_threads.is_empty() || !detail.issue_comments.is_empty();
    if !has_comments {
        return (Vec::new(), Vec::new());
    }
    let (comment_lines, _unresolved, alt_ranges) =
        comments_lines(detail, comments_expanded, p, ascii);
    (comment_lines, alt_ranges)
}
