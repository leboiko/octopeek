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

/// Width, in columns, used when rendering section dividers. Chosen wide
/// enough to look intentional on a 80-column terminal but not so wide that
/// it wraps on 60-col narrow terminals.
const DIVIDER_WIDTH: usize = 60;

/// Render one review thread as a contiguous block of `Line`s — header,
/// optional diff-hunk excerpt, then each comment's author line + body +
/// per-comment truncation marker. Reused by the ACTIVE and OUTDATED passes
/// in `comments_lines` so the two can't drift.
///
/// Promoted to `pub(super)` so `thread_card` can call it when rendering an
/// expanded inline thread card inside the diff view.
pub(super) fn render_thread_body(
    thread: &crate::github::detail::ReviewThread,
    expanded: bool,
    gutter: &'static str,
    reply_glyph: &'static str,
    p: &Palette,
    ascii: bool,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();

    // Thread header: `  ⚑ src/foo.rs:42  ·  2 comments  ·  unresolved`
    out.push(thread_header_line(thread, p, ascii));

    // Inline code excerpt from GitHub's `diffHunk`; empty when absent.
    let hunk_lines = diff_hunk_excerpt(thread.diff_hunk.as_deref(), p);
    if !hunk_lines.is_empty() {
        out.extend(hunk_lines);
        out.push(Line::from(""));
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
        out.push(author_line);

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
        out.extend(body_lines);

        if truncated {
            out.push(Line::from(vec![
                Span::styled(gutter, Style::default().fg(gutter_fg)),
                Span::styled("[m] expand", Style::default().fg(p.dim)),
            ]));
        }

        // Blank gutter line between comments within the same thread.
        if idx + 1 < thread.comments.len() {
            out.push(Line::from(vec![Span::styled(gutter, Style::default().fg(p.accent_alt))]));
        }
    }

    out
}

/// Override the foreground colour of every span in every line to the
/// supplied `muted` colour. Background, modifiers, and line layout are
/// preserved. Used to visibly de-emphasise outdated threads without hiding
/// them — the thread is still readable, just clearly subordinate to the
/// active ones above. Lossy for syntax-highlighted code blocks inside
/// outdated comment bodies; that's an acceptable tradeoff for "this
/// discussion no longer applies to the current diff".
fn mute_lines(lines: Vec<Line<'static>>, muted: ratatui::style::Color) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|mut line| {
            for span in &mut line.spans {
                span.style = span.style.fg(muted);
            }
            line
        })
        .collect()
}

/// Build a section divider line: `━━━━ LABEL (N) ━━━━` (or a dashed variant
/// for outdated threads). Used to split the Comments section into ACTIVE
/// and OUTDATED groups so outdated threads are visible but clearly
/// de-emphasised rather than silently dropped.
fn section_divider(
    label: &str,
    count: usize,
    rule_glyph: char,
    rule_color: ratatui::style::Color,
    ascii: bool,
) -> Line<'static> {
    let rule = if ascii { '-' } else { rule_glyph };
    let label_text = format!(" {label} ({count}) ");
    let rule_width = DIVIDER_WIDTH.saturating_sub(label_text.chars().count()) / 2;
    let rule_str: String = std::iter::repeat_n(rule, rule_width).collect();
    Line::from(vec![
        Span::styled(rule_str.clone(), Style::default().fg(rule_color)),
        Span::styled(label_text, Style::default().fg(rule_color).add_modifier(Modifier::BOLD)),
        Span::styled(rule_str, Style::default().fg(rule_color)),
    ])
}

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

    let mut spans = vec![
        Span::styled(format!("  {glyph} "), Style::default().fg(glyph_color)),
        Span::styled(location, Style::default().fg(p.accent)),
        Span::styled(count_str, Style::default().fg(p.dim)),
    ];
    // Show a prominent `[OUTDATED]` badge in `danger` so the thread's state
    // reads at a glance, not just via the muted status word at the end.
    // GitHub's web UI renders a yellow chip for the same reason.
    if thread.is_outdated {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            "[OUTDATED]".to_owned(),
            Style::default().fg(p.danger).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(status_str, Style::default().fg(p.dim)));

    Line::from(spans)
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
    show_outdated: bool,
    p: &Palette,
    ascii: bool,
) -> (Vec<Line<'static>>, Vec<u16>, Vec<(u16, u16)>) {
    let gutter = thread_gutter(ascii);
    let reply_glyph = if ascii { "> " } else { "\u{21b3} " };
    let unresolved_count =
        detail.review_threads.iter().filter(|t| !t.is_resolved && !t.is_outdated).count();
    let total_threads = detail.review_threads.len();
    let total_comments = detail.issue_comments.len();

    let mut lines = Vec::new();
    let mut unresolved_offsets: Vec<u16> = Vec::new();
    let mut alt_bg_ranges: Vec<(u16, u16)> = Vec::new();

    // Partition active vs outdated. Within each, sort unresolved first so
    // the next-unresolved-thread navigation key lands on the most-relevant
    // thread at the top.
    let (mut active, mut outdated): (
        Vec<&crate::github::detail::ReviewThread>,
        Vec<&crate::github::detail::ReviewThread>,
    ) = detail.review_threads.iter().partition(|t| !t.is_outdated);
    active.sort_by_key(|t| t.is_resolved);
    outdated.sort_by_key(|t| t.is_resolved);

    // When collapsed, allow up to 10 total items (threads + issue comments).
    let max_items = if expanded { usize::MAX } else { 10 };
    let mut items_shown = 0;
    // Toggle per top-level item (thread or standalone issue comment) so every
    // other conversation gets a subtle bg tint the user can group visually.
    let mut alt_on = false;

    // Render one section (active or outdated). Shared logic so the two
    // passes can't drift. Returns `(items_consumed)` for budget tracking.
    let render_section = |threads: &[&crate::github::detail::ReviewThread],
                          is_outdated_section: bool,
                          lines: &mut Vec<Line<'static>>,
                          alt_bg_ranges: &mut Vec<(u16, u16)>,
                          unresolved_offsets: &mut Vec<u16>,
                          items_shown: &mut usize,
                          alt_on: &mut bool| {
        for thread in threads {
            if *items_shown >= max_items {
                break;
            }

            // Only include non-outdated unresolved threads in the `n`/`N`
            // jumplist — outdated threads are informational and should not
            // steal navigation focus from open discussions.
            if !is_outdated_section && !thread.is_resolved {
                #[allow(clippy::cast_possible_truncation)]
                unresolved_offsets.push(lines.len() as u16);
            }

            let alt_start = lines.len();
            let thread_body = render_thread_body(thread, expanded, gutter, reply_glyph, p, ascii);
            // Outdated threads render at `palette.muted` so they're visibly
            // subordinate to the ACTIVE section while still readable.
            let thread_body =
                if is_outdated_section { mute_lines(thread_body, p.muted) } else { thread_body };
            lines.extend(thread_body);

            push_alt_range(alt_bg_ranges, alt_start, lines.len(), *alt_on);
            *alt_on = !*alt_on;

            lines.push(Line::from("")); // blank separator between threads
            *items_shown += 1;
        }
    };

    // ── Active threads ────────────────────────────────────────────────────────
    if !active.is_empty() {
        lines.push(section_divider("ACTIVE", active.len(), '\u{2501}', p.border_focused, ascii));
        render_section(
            &active,
            false,
            &mut lines,
            &mut alt_bg_ranges,
            &mut unresolved_offsets,
            &mut items_shown,
            &mut alt_on,
        );
    }

    // ── Outdated threads ──────────────────────────────────────────────────────
    // Silent-drop is a documented TUI anti-pattern (see `octo.nvim`); we keep
    // outdated threads visible-but-muted by default. `z` (the `show_outdated`
    // toggle) lets the user collapse them when the list gets noisy.
    if !outdated.is_empty() {
        if show_outdated {
            lines.push(section_divider("OUTDATED", outdated.len(), '\u{254C}', p.muted, ascii));
            render_section(
                &outdated,
                true,
                &mut lines,
                &mut alt_bg_ranges,
                &mut unresolved_offsets,
                &mut items_shown,
                &mut alt_on,
            );
        } else {
            // Disclosure: keep the section split visible even when hidden so
            // the user knows outdated threads exist and how to show them.
            lines.push(section_divider("OUTDATED", outdated.len(), '\u{254C}', p.muted, ascii));
            lines.push(Line::from(Span::styled(
                format!(
                    "  {} outdated thread{} hidden  \u{00B7}  [z] show",
                    outdated.len(),
                    if outdated.len() == 1 { "" } else { "s" }
                ),
                Style::default().fg(p.muted),
            )));
            lines.push(Line::from(""));
        }
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
    show_outdated: bool,
    p: &Palette,
    ascii: bool,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    let has_comments = !detail.review_threads.is_empty() || !detail.issue_comments.is_empty();
    if !has_comments {
        return (Vec::new(), Vec::new());
    }
    let (comment_lines, _unresolved, alt_ranges) =
        comments_lines(detail, comments_expanded, show_outdated, p, ascii);
    (comment_lines, alt_ranges)
}
