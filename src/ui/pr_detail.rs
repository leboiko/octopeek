//! PR detail panel — renders all sections for a single pull request.
//!
//! Layout (vertically scrollable):
//! 1. Banner (primary action flag, if not Clean/Draft)
//! 2. Title
//! 3. Meta line (author, age, commits, diff stats, comments)
//! 4. Body (rendered Markdown)
//! 5. CHECKS section
//! 6. REVIEWS section
//! 7. FILES CHANGED section
//! 8. COMMENTS section
//!
//! ## Thread hierarchy contract
//!
//! `comments_lines` renders review threads with a vertical `│` gutter so the
//! reader can see at a glance that all comments belong to one conversation.
//! The first comment in a thread is the opener; subsequent comments are prefixed
//! with `↳ ` in `palette.dim` to signal "this is a reply".
//!
//! `unresolved_anchors` returned by `build_content` always point at the
//! thread-header line (the `⚑/✓/◆ path:line` line), never at a comment body
//! line, so `n`/`N` navigation lands the reader at the right place.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::github::detail::{DetailedCheck, FileChangeKind, PrDetail, ReviewThread};
use crate::github::types::ReviewState;
use crate::ui::markdown::render_markdown;
use crate::ui::util::{humanize_delta, truncate};

// ── Section anchor bookkeeping ────────────────────────────────────────────────

/// Y-offsets (relative to the content top) where each major section begins.
///
/// Computed fresh every render frame so they stay accurate as content wraps.
///
/// Stored on [`App`] via [`crate::app::App::pr_detail_section_anchors`] and
/// written out by `collect_anchors` during rendering.
pub type SectionAnchors = Vec<u16>;

// ── Check run helpers ─────────────────────────────────────────────────────────

/// `true` when a check conclusion indicates failure that the viewer must fix.
fn check_is_failing(check: &DetailedCheck) -> bool {
    matches!(
        check.conclusion.as_deref(),
        Some("FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED")
    )
}

/// Glyph for a single check run: `✔`, `✖`, `●`, or `—`.
fn check_glyph(check: &DetailedCheck) -> &'static str {
    match check.conclusion.as_deref() {
        Some("SUCCESS") => "\u{2714}", // ✔
        Some("FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED") => {
            "\u{2716}" // ✖
        }
        None if check.status != "COMPLETED" => "\u{25CF}", // ● in-progress
        _ => "\u{2014}",                                   // — no conclusion on completed
    }
}

/// Format `duration_seconds` as `Xm Ys`.
fn fmt_duration(secs: u64) -> String {
    if secs < 60 { format!("{secs}s") } else { format!("{}m {}s", secs / 60, secs % 60) }
}

// ── File change helpers ───────────────────────────────────────────────────────

fn file_kind_glyph(kind: FileChangeKind) -> &'static str {
    match kind {
        FileChangeKind::Added => "\u{271A}",    // ✚
        FileChangeKind::Modified => "\u{270E}", // ✎
        FileChangeKind::Deleted => "\u{2702}",  // ✂
        FileChangeKind::Renamed => "\u{2192}",  // →
        FileChangeKind::Copied | FileChangeKind::Changed => "\u{00B7}", // ·
    }
}

// ── Sticky header ─────────────────────────────────────────────────────────────

/// Short state label + color for the sticky header's top line.
///
/// The [`PrDetail`] model does not currently distinguish "closed unmerged" from
/// "open", so we treat any non-draft, non-merged PR as OPEN. If closed-state
/// ever shows up in the model, this is the single place to teach it.
fn pr_state_label(detail: &PrDetail, p: &crate::theme::Palette) -> (&'static str, Color) {
    if detail.merged {
        ("MERGED", p.accent_alt)
    } else if detail.is_draft {
        ("DRAFT", p.dim)
    } else {
        ("OPEN", p.success)
    }
}

/// Build the sticky header lines for a PR.
///
/// The header is rendered in its own fixed-height region above the scrolling
/// body so the reader never loses the repo/number/title/stats context. Returns
/// one `Line` per visible row; callers use `len()` for layout sizing.
///
/// Layout:
/// 1. `repo #N  ·  STATE  ·  @author opened AGE` (dim, STATE coloured)
/// 2. Title (bold foreground)
/// 3. `head → base  ·  +A −D across N files  ·  C comments` (dim)
/// 4. `✖ CI FAILING` (danger) — only when the banner signal fires
pub fn build_header(detail: &PrDetail, p: &crate::theme::Palette) -> Vec<Line<'static>> {
    let (state_text, state_color) = pr_state_label(detail, p);
    let age = humanize_delta(&detail.created_at);

    // Line 1: repo + number + state + author + age.
    let line1 = Line::from(vec![
        Span::styled(
            format!("{} #{}", detail.repo, detail.number),
            Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  \u{00B7}  ", Style::default().fg(p.dim)),
        Span::styled(
            state_text,
            Style::default().fg(state_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  \u{00B7}  ", Style::default().fg(p.dim)),
        Span::styled(format!("@{}", detail.author), Style::default().fg(p.foreground)),
        Span::styled(format!(" opened {age}"), Style::default().fg(p.dim)),
    ]);

    // Line 2: title.
    let line2 = Line::from(Span::styled(
        detail.title.clone(),
        Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
    ));

    // Line 3: branches + diff stats + comment count.
    let line3 = Line::from(vec![
        Span::styled(detail.head_ref.clone(), Style::default().fg(p.accent_alt)),
        Span::styled(" \u{2192} ", Style::default().fg(p.dim)), // →
        Span::styled(detail.base_ref.clone(), Style::default().fg(p.accent_alt)),
        Span::styled("  \u{00B7}  ", Style::default().fg(p.dim)),
        Span::styled(format!("+{}", detail.additions), Style::default().fg(p.git_new)),
        Span::styled(" ", Style::default()),
        Span::styled(
            format!("\u{2212}{}", detail.deletions), // −
            Style::default().fg(p.danger),
        ),
        Span::styled(
            format!("  across {} files  \u{00B7}  {} comments", detail.changed_files_count, detail.issue_comments.len()),
            Style::default().fg(p.dim),
        ),
    ]);

    let mut header = vec![line1, line2, line3];
    if let Some(banner) = banner_line(detail, p) {
        header.push(banner);
    }
    header
}

// ── Banner line ───────────────────────────────────────────────────────────────

/// Produce the flag banner line (may be empty) for the top of the detail view.
fn banner_line(detail: &PrDetail, p: &crate::theme::Palette) -> Option<Line<'static>> {
    // Derive a simplified flag directly from PrDetail fields (we don't have a
    // list-level PullRequest here, but the high-priority signals are present).
    if detail.is_draft {
        return None; // Draft: no urgent banner
    }
    if detail.merged {
        return None; // Already merged: nothing urgent
    }

    // Check for failing checks.
    let has_failing = detail.check_runs.iter().any(check_is_failing);
    if has_failing {
        return Some(Line::from(Span::styled(
            "\u{2716} CI FAILING".to_owned(),
            Style::default().fg(p.danger).add_modifier(Modifier::BOLD),
        )));
    }

    None
}

// ── Section header helper ─────────────────────────────────────────────────────

fn section_header(label: &str, p: &crate::theme::Palette) -> Line<'static> {
    // Thicker leading rule + bold label gives each section a clear visual
    // break from the paragraph text above. We intentionally keep it on one
    // line (no trailing rule) so very long labels like "COMMENTS (42 threads
    // · 7 unresolved)" stay legible instead of wrapping mid-rule.
    let rule = "\u{2501}".repeat(3); // ━━━
    Line::from(vec![
        Span::styled(
            format!("{rule} "),
            Style::default().fg(p.accent),
        ),
        Span::styled(
            label.to_owned(),
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Apply the tint to every span of `line` and right-pad to `width` cells,
/// so short lines render as a solid tinted rectangle.
///
/// Used in copy mode where line-wrap is disabled and the logical line maps
/// one-to-one to a screen row. Outside copy mode, [`char_wrap_tint`] is
/// used instead so long content wraps into multiple fully-tinted rows.
fn tint_line(line: &Line<'static>, bg: Color, width: u16) -> Line<'static> {
    let current_width: usize =
        line.spans.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
    let target = usize::from(width);
    let pad = target.saturating_sub(current_width);

    let mut spans: Vec<Span<'static>> = line
        .spans
        .iter()
        .map(|s| Span::styled(s.content.clone(), s.style.bg(bg)))
        .collect();
    if pad > 0 {
        spans.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
    }
    let mut result = Line::from(spans);
    result.style = Style::default().bg(bg);
    result
}

/// Pre-wrap `line` at character boundaries into one-or-more `Line`s, each
/// exactly `width` cells wide and fully tinted with `bg`.
///
/// This side-steps the ratatui-Paragraph-word-wrap problem that caused the
/// patchy tint bug: when a line is longer than `width` we split it
/// ourselves (no word-boundary gaps) and emit multiple lines that do not
/// need further wrapping, so every visual row of the tinted range is
/// completely filled with the tint — not just the cells containing text.
///
/// Uses display width (unicode-width) so CJK and emoji don't blow through
/// the column budget.
fn char_wrap_tint(line: &Line<'static>, bg: Color, width: u16) -> Vec<Line<'static>> {
    let w = usize::from(width).max(1);
    let bg_style = Style::default().bg(bg);

    let mut out: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut current_w = 0usize;

    for span in &line.spans {
        let tinted = span.style.bg(bg);
        // Fast path: the whole span fits in the remainder of the current row.
        let span_w = UnicodeWidthStr::width(span.content.as_ref());
        if current_w + span_w <= w {
            current.push(Span::styled(span.content.clone(), tinted));
            current_w += span_w;
            continue;
        }
        // Slow path: walk the span char-by-char, splitting at the column
        // boundary. `buf` accumulates the in-progress piece of this span so
        // we emit a single Span per contiguous run, not one per character.
        let mut buf = String::new();
        let mut buf_w = 0usize;
        for ch in span.content.chars() {
            // Treat zero-width chars as width 1 so they still make progress
            // and don't create infinite loops in pathological inputs.
            let cw = UnicodeWidthStr::width(ch.to_string().as_str()).max(1);
            if current_w + buf_w + cw > w {
                // Emit accumulated buf into the current line, then flush.
                if !buf.is_empty() {
                    current.push(Span::styled(std::mem::take(&mut buf), tinted));
                    current_w += buf_w;
                    buf_w = 0;
                }
                flush_tinted_line(&mut current, current_w, w, bg_style, &mut out);
                current_w = 0;
            }
            buf.push(ch);
            buf_w += cw;
        }
        if !buf.is_empty() {
            current.push(Span::styled(buf, tinted));
            current_w += buf_w;
        }
    }

    flush_tinted_line(&mut current, current_w, w, bg_style, &mut out);
    out
}

/// Push `current` as a finished, `width`-cell tinted line and reset.
/// Factored out to keep [`char_wrap_tint`] under the pedantic line limit.
fn flush_tinted_line(
    current: &mut Vec<Span<'static>>,
    current_w: usize,
    width: usize,
    bg_style: Style,
    out: &mut Vec<Line<'static>>,
) {
    let pad = width.saturating_sub(current_w);
    if pad > 0 {
        current.push(Span::styled(" ".repeat(pad), bg_style));
    }
    let mut line = Line::from(std::mem::take(current));
    line.style = bg_style;
    out.push(line);
}

/// Return a copy of `lines` with `alt_bg` applied to every line whose index
/// falls within any `(start, end)` half-open range in `alt_ranges`.
///
/// `wrap_enabled` controls the tinting strategy for lines longer than
/// `width`:
/// - `true` (normal view): pre-wrap at character boundaries into multiple
///   fully-tinted lines. Safe because ratatui's Paragraph won't re-wrap a
///   line that already fits its target width.
/// - `false` (copy mode): just pad short lines. Long lines extend past the
///   viewport and the horizontal scroll offset slides them around; no
///   wrapping happens, so the tint is never broken across rows.
///   Critically, preserving the 1:1 mapping between `content_lines` indices
///   and rendered lines keeps the copy-mode cursor row math correct.
fn apply_alt_bg(
    lines: &[Line<'static>],
    alt_ranges: &[(u16, u16)],
    bg: Color,
    width: u16,
    wrap_enabled: bool,
) -> Vec<Line<'static>> {
    if alt_ranges.is_empty() || width == 0 {
        return lines.to_vec();
    }
    let mut out: Vec<Line<'static>> = Vec::with_capacity(lines.len());
    for (idx, line) in lines.iter().enumerate() {
        let in_range = alt_ranges.iter().any(|&(a, b)| {
            let a = usize::from(a);
            let b = usize::from(b);
            idx >= a && idx < b
        });
        if !in_range {
            out.push(line.clone());
        } else if wrap_enabled {
            out.extend(char_wrap_tint(line, bg, width));
        } else {
            out.push(tint_line(line, bg, width));
        }
    }
    out
}

// ── Section renderers ─────────────────────────────────────────────────────────

/// Build check-run lines (up to 8, with overflow footer).
fn checks_lines(detail: &PrDetail, p: &crate::theme::Palette) -> Vec<Line<'static>> {
    let mut checks: Vec<&DetailedCheck> = detail.check_runs.iter().collect();
    // Failing checks sorted first.
    checks.sort_by_key(|c| !check_is_failing(c));

    let visible = checks.len().min(8);
    let overflow = checks.len().saturating_sub(8);

    let mut lines = Vec::with_capacity(visible + 1);
    for check in &checks[..visible] {
        let glyph = check_glyph(check);
        let glyph_color = if check_is_failing(check) {
            p.danger
        } else if check.conclusion.as_deref() == Some("SUCCESS") {
            p.success
        } else {
            p.muted
        };

        let workflow_prefix =
            check.workflow_name.as_deref().map(|wf| format!("{wf} / ")).unwrap_or_default();

        let duration_str =
            check.duration_seconds.map(|s| format!(" ({})", fmt_duration(s))).unwrap_or_default();

        let status_text = check.conclusion.as_deref().unwrap_or(&check.status).to_lowercase();

        lines.push(Line::from(vec![
            Span::styled(format!("{glyph} "), Style::default().fg(glyph_color)),
            Span::styled(workflow_prefix, Style::default().fg(p.dim)),
            Span::styled(check.name.clone(), Style::default().fg(p.foreground)),
            Span::styled(format!(" [{status_text}]"), Style::default().fg(p.muted)),
            Span::styled(duration_str, Style::default().fg(p.dim)),
        ]));
    }

    if overflow > 0 {
        lines.push(Line::from(Span::styled(
            format!("  ... {overflow} more"),
            Style::default().fg(p.dim),
        )));
    }

    lines
}

/// Build review lines (one or two lines per review).
fn reviews_lines(detail: &PrDetail, p: &crate::theme::Palette) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for review in &detail.reviews {
        let (verdict, color) = match review.state {
            ReviewState::Approved => ("approved", p.success),
            ReviewState::ChangesRequested => ("changes requested", p.danger),
            ReviewState::Commented => ("commented", p.muted),
            ReviewState::Dismissed => ("dismissed (dismissed)", p.muted),
            ReviewState::Pending => ("pending", p.dim),
        };

        let age = humanize_delta(&review.submitted_at);
        lines.push(Line::from(vec![
            Span::styled("\u{25CF} ", Style::default().fg(color)), // ●
            Span::styled(format!("@{}", review.author), Style::default().fg(p.foreground)),
            Span::styled(format!(" {verdict}"), Style::default().fg(color)),
            Span::styled(format!(" {age}"), Style::default().fg(p.dim)),
        ]));

        // If the review has a body, show first 80 chars truncated in dim.
        let body = review.body_markdown.trim();
        if !body.is_empty() {
            let first_line = body.lines().next().unwrap_or(body);
            lines.push(Line::from(Span::styled(
                format!("    {}", truncate(first_line, 80)),
                Style::default().fg(p.dim),
            )));
        }
    }
    lines
}

/// Build files-changed lines, with expansion controlled by `expanded`.
fn files_lines(detail: &PrDetail, expanded: bool, p: &crate::theme::Palette) -> Vec<Line<'static>> {
    // Sort by magnitude (additions + deletions) descending.
    let mut files: Vec<&crate::github::detail::FileChange> = detail.files.iter().collect();
    files.sort_by(|a, b| {
        let mag_b = b.additions + b.deletions;
        let mag_a = a.additions + a.deletions;
        mag_b.cmp(&mag_a)
    });

    let visible = if expanded { files.len() } else { files.len().min(5) };
    let overflow = files.len().saturating_sub(5);

    let mut lines = Vec::with_capacity(visible + 1);
    for file in &files[..visible] {
        let glyph = file_kind_glyph(file.change_kind);
        let glyph_color = match file.change_kind {
            FileChangeKind::Added => p.success,
            FileChangeKind::Modified => p.warning,
            FileChangeKind::Deleted => p.danger,
            FileChangeKind::Renamed => p.accent,
            FileChangeKind::Copied | FileChangeKind::Changed => p.muted,
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{glyph} "), Style::default().fg(glyph_color)),
            Span::styled(file.path.clone(), Style::default().fg(p.foreground)),
            Span::styled(format!(" +{}", file.additions), Style::default().fg(p.success)),
            Span::styled(format!(" \u{2212}{}", file.deletions), Style::default().fg(p.danger)),
        ]));
    }

    if !expanded && overflow > 0 {
        lines.push(Line::from(Span::styled(
            format!("  ... {overflow} more  [f] to expand"),
            Style::default().fg(p.dim),
        )));
    }

    lines
}

// ── Gutter helper ─────────────────────────────────────────────────────────────

/// The Unicode vertical gutter prepended to every body line inside a review thread.
const THREAD_GUTTER_UNICODE: &str = "  \u{2502}  "; // "  │  "

/// The ASCII fallback used when `config.show_ascii_glyphs` is true; some
/// terminals (older `PuTTY`, ssh through limited charsets) render Unicode box
/// drawing as replacement squares.
const THREAD_GUTTER_ASCII: &str = "  |  ";

/// Return the gutter string appropriate for the current `ascii` setting.
fn thread_gutter(ascii: bool) -> &'static str {
    if ascii { THREAD_GUTTER_ASCII } else { THREAD_GUTTER_UNICODE }
}

/// Wrap rendered markdown lines with the thread gutter prefix.
///
/// Each `Line` from `render_markdown` gets a leading gutter span prepended,
/// coloured with `gutter_fg`. The opener uses the default
/// `palette.block_quote_border`; replies use a distinct colour (normally
/// `palette.accent_alt`) so the reply's vertical rail visually separates
/// it from the thread opener sitting right above.
///
/// When the incoming line's first span has a background color (typical for
/// syntect-highlighted code-block lines), the gutter span inherits that
/// background so the code-block's colored rail extends cleanly through the
/// gutter column instead of breaking to the terminal default.
fn gutter_lines(
    md_lines: Vec<Line<'static>>,
    gutter_fg: Color,
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
///
/// Used in two places:
/// - Top-level issue comments (2-space indent, no gutter — there is no
///   conversation to tether).
/// - Reply body lines inside a review thread (2-space indent INSIDE the
///   existing gutter so replies visually step in from the thread opener).
fn indent_lines(md_lines: Vec<Line<'static>>, prefix: &'static str) -> Vec<Line<'static>> {
    md_lines
        .into_iter()
        .map(|mut line| {
            line.spans.insert(0, Span::raw(prefix));
            line
        })
        .collect()
}

// ── Thread header ─────────────────────────────────────────────────────────────

/// Build the single header line for a review thread:
/// `  <glyph> <path>:<line>  ·  <N> comments  ·  <status>`
///
/// - Unresolved: `⚑` / `!` in `palette.warning`
/// - Resolved: `✓` / `+` in `palette.muted`
/// - Outdated: `◆` / `D` in `palette.muted`
///
/// The ASCII fallback is selected when `config.show_ascii_glyphs == true` so
/// terminals without a Unicode font do not render replacement squares.
fn thread_header_line(
    thread: &ReviewThread,
    p: &crate::theme::Palette,
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
/// ## Thread contract
///
/// For each review thread the layout is:
/// 1. Thread header line (`⚑/✓/◆ path:line · N comments · status`)
/// 2. For each comment in the thread:
///    - Author-age line, prefixed by `"  │  "` gutter (and `"↳ "` for replies)
///    - Rendered markdown body lines, each prefixed by `"  │  "` gutter
///    - Blank gutter line separating comments within the same thread
/// 3. Blank line (no gutter) between threads
///
/// ## Returns
///
/// `(lines, unresolved_thread_relative_offsets)` where offsets are relative
/// to the start of the comment block (0 = first line of the block header).
/// Callers add the header's absolute Y to get global anchors.
///
/// `unresolved_anchors` always point at the thread-header line so `n`/`N`
/// navigation lands correctly regardless of body content.
#[allow(clippy::too_many_lines)]
fn comments_lines(
    detail: &PrDetail,
    expanded: bool,
    p: &crate::theme::Palette,
    ascii: bool,
) -> (Vec<Line<'static>>, Vec<u16>, Vec<(u16, u16)>) {
    let gutter = thread_gutter(ascii);
    let reply_glyph = if ascii { "> " } else { "\u{21b3} " };
    let unresolved_count = detail.review_threads.iter().filter(|t| !t.is_resolved).count();
    let total_threads = detail.review_threads.len();
    let total_comments = detail.issue_comments.len();

    let mut lines = Vec::new();
    // Track relative Y of each unresolved thread header (within this block, after the section
    // header). These are shifted by +1 when the section header is prepended.
    let mut unresolved_offsets: Vec<u16> = Vec::new();
    // Alternating-bg line ranges, also header-shifted by the caller.
    let mut alt_bg_ranges: Vec<(u16, u16)> = Vec::new();

    // Sort threads: unresolved first.
    let mut threads: Vec<&ReviewThread> = detail.review_threads.iter().collect();
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
        // navigation jumps to the right line. `lines.len()` at this point is
        // the 0-based index of the header line within this block.
        if !thread.is_resolved {
            #[allow(clippy::cast_possible_truncation)]
            unresolved_offsets.push(lines.len() as u16);
        }

        // Mark where this top-level item begins so the draw-time tint covers
        // exactly the thread's lines (header + all comments + intra-thread
        // blank gutter rows) — not the trailing blank separator that lives
        // between threads.
        let alt_start = lines.len();

        // Thread header: `  ⚑ src/foo.rs:42  ·  2 comments  ·  unresolved`
        lines.push(thread_header_line(thread, p, ascii));

        for (idx, comment) in thread.comments.iter().enumerate() {
            let age = humanize_delta(&comment.created_at);

            // The first comment is the thread opener; subsequent ones are
            // replies. Replies get a distinct colour treatment so the reader
            // can tell at a glance where the conversation picks up:
            //   - `↳` glyph + author in `palette.accent_alt` (brighter than
            //     the foreground, reads as a deliberate visual hook).
            //   - The vertical `│` gutter rail for the reply's body is also
            //     tinted with `accent_alt`, producing a coloured "sidebar"
            //     that frames the reply block.
            //   - Body content keeps its normal markdown styling so inline
            //     code / links / bold still render as the user expects.
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
            // When collapsed, cap each comment at 6 rendered lines and append an
            // expand hint. When expanded, show the full body.
            let body = comment.body_markdown.trim();
            let rendered = render_markdown(body, p);
            let total_rendered = rendered.len();

            let (visible_rendered, truncated) = if !expanded && total_rendered > 6 {
                (rendered.into_iter().take(6).collect::<Vec<_>>(), true)
            } else {
                (rendered, false)
            };

            // Gutter first. For replies, also step-in the body by 2 spaces so
            // a reply's long markdown body is obviously offset from the prior
            // comment's body (which sits flush against the gutter). The reply
            // gutter rail carries the accent colour so the whole reply block
            // reads as visually distinct from the opener above.
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

            // Blank gutter line between comments within the same thread so
            // code blocks don't blur into each other. The rail colour here
            // matches the NEXT comment (always a reply by construction —
            // only idx 0 is the opener), so the visual break lines up with
            // the coloured reply rail that follows.
            if idx + 1 < thread.comments.len() {
                lines.push(Line::from(vec![Span::styled(
                    gutter,
                    Style::default().fg(p.accent_alt),
                )]));
            }
        }

        // Close the alt-bg range BEFORE the trailing blank separator so the
        // tint stops at the last content row, not in the gap.
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

        // Body rendered as markdown, indented by two spaces (no gutter — these
        // are top-level comments with nothing to tether).
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

/// Alternating-bg range helper: record `(start_line_idx, end_line_idx_exclusive)`
/// for the lines belonging to a single top-level comment/thread when the
/// current parity calls for a tint. Returning ranges (vs a bitset) keeps the
/// draw-time tint + right-pad math trivial.
fn push_alt_range(ranges: &mut Vec<(u16, u16)>, start: usize, end: usize, alt_on: bool) {
    if !alt_on || end <= start {
        return;
    }
    let start = u16::try_from(start).unwrap_or(u16::MAX);
    let end = u16::try_from(end).unwrap_or(u16::MAX);
    ranges.push((start, end));
}

// ── Top-level content builder ─────────────────────────────────────────────────

/// Build all content lines for the PR detail view.
///
/// Returns `(lines, section_anchors, unresolved_thread_anchors)` where anchors
/// are absolute Y offsets within the content.
///
/// `unresolved_thread_anchors` always point at the thread-header line for each
/// unresolved thread so `n`/`N` navigation in the key handler works correctly.
#[allow(clippy::type_complexity)] // Single public API; a named struct would obscure callers more than clarify.
pub fn build_content(
    detail: &PrDetail,
    files_expanded: bool,
    comments_expanded: bool,
    p: &crate::theme::Palette,
    ascii: bool,
) -> (Vec<Line<'static>>, SectionAnchors, Vec<u16>, Vec<(u16, u16)>) {
    let mut all_lines: Vec<Line<'static>> = Vec::new();
    let mut section_anchors: SectionAnchors = Vec::new();
    let mut unresolved_anchors: Vec<u16> = Vec::new();
    let mut alt_bg_ranges: Vec<(u16, u16)> = Vec::new();

    // The banner, title, and meta lines used to live at the top of the
    // scrolling content. They are now rendered by `build_header` into a fixed
    // region above the body, so the body starts directly with the description
    // markdown. Tab navigation anchors accordingly begin at BODY / CHECKS /
    // whichever section is the first with content.

    // ── Body (rendered Markdown) ───────────────────────────────────────────────
    // The body gets its own anchor (even when empty) so Tab navigation always
    // has a "top" target to jump to. This preserves the behaviour of the old
    // banner/title anchors for users who rely on `gg` or Shift+Tab-to-top.
    #[allow(clippy::cast_possible_truncation)]
    let body_anchor = all_lines.len() as u16;
    section_anchors.push(body_anchor);
    if !detail.body_markdown.is_empty() {
        let body_lines = render_markdown(&detail.body_markdown, p);
        all_lines.extend(body_lines);
        all_lines.push(Line::from(""));
    }

    // ── CHECKS ───────────────────────────────────────────────────────────────
    if !detail.check_runs.is_empty() {
        #[allow(clippy::cast_possible_truncation)]
        let checks_anchor = all_lines.len() as u16;
        section_anchors.push(checks_anchor);
        all_lines.push(section_header("CHECKS", p));
        all_lines.extend(checks_lines(detail, p));
        all_lines.push(Line::from(""));
    }

    // ── REVIEWS ───────────────────────────────────────────────────────────────
    if !detail.reviews.is_empty() {
        #[allow(clippy::cast_possible_truncation)]
        let reviews_anchor = all_lines.len() as u16;
        section_anchors.push(reviews_anchor);
        all_lines.push(section_header("REVIEWS", p));
        all_lines.extend(reviews_lines(detail, p));
        all_lines.push(Line::from(""));
    }

    // ── FILES CHANGED ─────────────────────────────────────────────────────────
    if !detail.files.is_empty() {
        #[allow(clippy::cast_possible_truncation)]
        let files_anchor = all_lines.len() as u16;
        section_anchors.push(files_anchor);
        all_lines
            .push(section_header(&format!("FILES CHANGED ({})", detail.changed_files_count), p));
        all_lines.extend(files_lines(detail, files_expanded, p));
        all_lines.push(Line::from(""));
    }

    // ── COMMENTS ─────────────────────────────────────────────────────────────
    let has_comments = !detail.review_threads.is_empty() || !detail.issue_comments.is_empty();
    if has_comments {
        #[allow(clippy::cast_possible_truncation)]
        let comments_anchor = all_lines.len() as u16;
        section_anchors.push(comments_anchor);
        let (comment_lines, thread_offsets, comment_alt_ranges) =
            comments_lines(detail, comments_expanded, p, ascii);
        // Convert thread relative offsets to absolute Y offsets.
        for offset in thread_offsets {
            unresolved_anchors.push(comments_anchor + offset);
        }
        for (a, b) in comment_alt_ranges {
            alt_bg_ranges.push((comments_anchor + a, comments_anchor + b));
        }
        all_lines.extend(comment_lines);
    }

    (all_lines, section_anchors, unresolved_anchors, alt_bg_ranges)
}

// ── draw (public entry point) ─────────────────────────────────────────────────

/// Render the PR detail panel into `area`.
///
/// Handles three states:
/// - Fetching (no detail yet): centered spinner text.
/// - Error (fetch failed): error panel with retry hint.
/// - Loaded: full scrollable detail layout.
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

    // Split the detail area into a fixed-height sticky header and the
    // scrollable body below it. The header stays visible no matter how far
    // the reader scrolls, so the repo/number/title/branches context never
    // drops out of view. A thin rule drawn by `render_pr_header` separates
    // the two regions so the tint band has a clean lower edge.
    let header_lines = build_header(detail, p);
    #[allow(clippy::cast_possible_truncation)]
    let header_rows = (header_lines.len() + 2) as u16; // +2 = top pad + bottom rule
    let header_rows = header_rows.min(area.height);
    let splits = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Length(header_rows),
        ratatui::layout::Constraint::Min(1),
    ])
    .split(area);
    let header_area = splits[0];
    let body_area = splits[1];

    render_pr_header(f, header_lines, header_area, p);

    let (content_lines, _section_anchors, _unresolved_anchors, alt_bg_ranges) = build_content(
        detail,
        app.pr_detail_files_expanded,
        app.pr_detail_comments_expanded,
        p,
        app.config.show_ascii_glyphs,
    );

    // Reading the detail in a terminal-wide column is visually hostile: long
    // lines turn into endless sentences and comment blocks blur into each
    // other. A padded inner rect creates a narrower reading column and a
    // gutter of breathing room on each side.
    let block = Block::default()
        .style(Style::default().bg(p.background).fg(p.foreground))
        .padding(Padding::new(2, 2, 0, 0));
    let inner = block.inner(body_area);

    // Cache the inner rect (not the outer area) so key handlers auto-scroll
    // based on the actual content viewport, and mouse coordinates map into
    // the content's coordinate system rather than landing on the pad columns.
    // Note: this is the *body* rect; the header is non-interactive (no copy
    // mode or cursor lands there), so header clicks are correctly ignored
    // by the bounds-check in `App::mouse_to_content_pos`.
    app.pr_detail_viewport.set(inner);

    let scroll = app.pr_detail_scroll;

    // Alternating comment tint. In the normal view we let `apply_alt_bg`
    // pre-wrap long tinted lines at character boundaries so each visual row
    // fills the viewport with the tint — ratatui's word-wrap used to leave
    // trailing cells of wrapped rows unpainted. In copy mode we keep the
    // 1:1 line mapping so the cursor row math stays honest; wrap is already
    // disabled in that branch, so long lines just extend off-screen with a
    // padded tint (fine, never actually broken into multiple rows).
    let tinted_lines = apply_alt_bg(
        &content_lines,
        &alt_bg_ranges,
        p.help_bg,
        inner.width,
        !app.copy_mode.active,
    );

    // In copy mode we disable wrapping so that logical lines map 1:1 to screen
    // lines — otherwise the cursor's logical column would not match any
    // predictable screen column. Horizontal scrolling kicks in for long lines.
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

    f.render_widget(widget, body_area);
}

/// Render the sticky PR header into `area`.
///
/// Wraps the caller-provided lines in a horizontally-padded block tinted
/// with `palette.help_bg` (same tone used for comment stripes, so the reader
/// sees the same "card" cue throughout the app). A bottom rule drawn in
/// `palette.accent` makes the boundary with the scrolling body unmistakable,
/// especially on themes where `help_bg` and `background` differ only slightly.
fn render_pr_header(
    f: &mut Frame,
    lines: Vec<Line<'static>>,
    area: Rect,
    p: &crate::theme::Palette,
) {
    if area.height == 0 {
        return;
    }
    // Split: content rows + one bottom rule row.
    let rule_row = area.height.saturating_sub(1);
    let content_h = area.height.saturating_sub(1);

    let content_area = Rect { x: area.x, y: area.y, width: area.width, height: content_h };
    let rule_area = Rect { x: area.x, y: area.y + rule_row, width: area.width, height: 1 };

    let block = Block::default()
        .style(Style::default().bg(p.help_bg).fg(p.foreground))
        .padding(Padding::new(2, 2, 1, 0));
    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, content_area);

    // Full-width rule. Uses a heavy box-drawing char so the separator reads
    // as a deliberate section break rather than a faint line artefact.
    let rule_text = "\u{2501}".repeat(usize::from(rule_area.width));
    let rule = Paragraph::new(Line::from(Span::styled(
        rule_text,
        Style::default().fg(p.accent).bg(p.background),
    )));
    f.render_widget(rule, rule_area);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used)]
pub mod tests {
    use super::*;
    use crate::github::detail::{
        DetailedCheck, DetailedReview, FileChange, FileChangeKind, IssueComment, PrDetail,
        ReviewComment, ReviewThread,
    };
    use crate::github::types::ReviewState;
    use crate::theme::Palette;
    use chrono::Utc;

    /// Build a fixture `PrDetail` with a configurable number of checks, reviews, and files.
    pub fn fixture_pr_detail(
        num_checks: usize,
        num_reviews: usize,
        num_files: usize,
        num_threads: usize,
    ) -> PrDetail {
        let now = Utc::now();

        let check_runs = (0..num_checks)
            .map(|i| DetailedCheck {
                name: format!("check-{i}"),
                workflow_name: Some("CI".to_owned()),
                status: "COMPLETED".to_owned(),
                conclusion: if i % 3 == 0 {
                    Some("FAILURE".to_owned())
                } else {
                    Some("SUCCESS".to_owned())
                },
                duration_seconds: Some(60 + i as u64 * 10),
                details_url: None,
            })
            .collect();

        let reviews = (0..num_reviews)
            .map(|i| DetailedReview {
                author: format!("reviewer-{i}"),
                state: if i % 2 == 0 {
                    ReviewState::Approved
                } else {
                    ReviewState::ChangesRequested
                },
                body_markdown: format!("Review body {i}"),
                submitted_at: now,
            })
            .collect();

        let files = (0..num_files)
            .map(|i| FileChange {
                path: format!("src/file-{i}.rs"),
                #[allow(clippy::cast_possible_truncation)]
                additions: (i as u32 + 1) * 10,
                #[allow(clippy::cast_possible_truncation)]
                deletions: i as u32 * 2,
                change_kind: if i % 2 == 0 {
                    FileChangeKind::Modified
                } else {
                    FileChangeKind::Added
                },
            })
            .collect();

        let review_threads = (0..num_threads)
            .map(|i| ReviewThread {
                path: format!("src/file-{i}.rs"),
                #[allow(clippy::cast_possible_truncation)]
                line: Some((i as u32 + 1) * 5),
                is_resolved: i % 3 == 0,
                is_outdated: false,
                comments: vec![ReviewComment {
                    author: format!("user-{i}"),
                    body_markdown: format!("Comment {i}"),
                    created_at: now,
                }],
            })
            .collect();

        PrDetail {
            repo: "owner/repo".to_owned(),
            number: 1,
            title: "Test PR".to_owned(),
            url: "https://github.com/owner/repo/pull/1".to_owned(),
            author: "alice".to_owned(),
            body_markdown: "## Summary\n\nThis is a test PR.".to_owned(),
            base_ref: "main".to_owned(),
            head_ref: "feat/test".to_owned(),
            is_draft: false,
            additions: 100,
            deletions: 50,
            #[allow(clippy::cast_possible_truncation)]
            changed_files_count: num_files as u32,
            updated_at: now,
            created_at: now,
            merged: false,
            files,
            check_runs,
            reviews,
            review_threads,
            issue_comments: vec![IssueComment {
                author: "carol".to_owned(),
                body_markdown: "Nice work!".to_owned(),
                created_at: now,
            }],
        }
    }

    /// Helper: concatenate all span text in a line.
    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// Section anchors must be monotonically non-decreasing and contain one
    /// entry for each major section that has content.
    #[test]
    fn section_anchors_are_monotone() {
        let detail = fixture_pr_detail(3, 2, 4, 2);
        let p = Palette::default();
        let (_, anchors, _, _) = build_content(&detail, false, false, &p, false);

        // We have: banner, title, checks, reviews, files, comments = 6 anchors.
        assert!(!anchors.is_empty(), "anchors should not be empty");

        // Each anchor must be >= the previous (monotone non-decreasing).
        for window in anchors.windows(2) {
            assert!(window[1] >= window[0], "anchors not monotone: {anchors:?}");
        }
    }

    /// Anchor list includes every body-side section that has content. Since
    /// the sticky header moved banner/title/meta out of the scrolling body,
    /// the first anchor is now BODY rather than the old BANNER.
    #[test]
    fn section_anchors_count_matches_content() {
        let detail = fixture_pr_detail(2, 1, 3, 1);
        let p = Palette::default();
        let (_, anchors, _, _) = build_content(&detail, false, false, &p, false);
        // body + checks + reviews + files + comments = 5
        assert_eq!(anchors.len(), 5, "expected 5 anchors for full fixture, got {}", anchors.len());
        assert_eq!(anchors[0], 0, "body anchor must be the top of the scroll buffer");
    }

    /// The sticky header must contain the repo/number, title, state label,
    /// and branch arrow — it's the landing pad that replaces the old
    /// in-body banner/title/meta trio, so regressions here are user-visible.
    #[test]
    fn build_header_contains_core_context() {
        let detail = fixture_pr_detail(0, 0, 0, 0);
        let p = Palette::default();
        let lines = build_header(&detail, &p);
        let text: String =
            lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(text.contains("owner/repo #1"), "repo/number missing: {text}");
        assert!(text.contains("OPEN"), "state label missing: {text}");
        assert!(text.contains("Test PR"), "title missing: {text}");
        assert!(text.contains("feat/test"), "head branch missing: {text}");
        assert!(text.contains("main"), "base branch missing: {text}");
    }

    /// Header state label must flip with `is_draft` / `merged` fields.
    #[test]
    fn build_header_state_label_reflects_state() {
        let p = Palette::default();
        let mut detail = fixture_pr_detail(0, 0, 0, 0);

        detail.is_draft = true;
        let text: String = build_header(&detail, &p)
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("DRAFT"), "draft label missing: {text}");

        detail.is_draft = false;
        detail.merged = true;
        let text: String = build_header(&detail, &p)
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("MERGED"), "merged label missing: {text}");
    }

    /// Unresolved thread anchors must be a subset of the total anchor range.
    #[test]
    fn unresolved_anchors_within_total_lines() {
        let detail = fixture_pr_detail(1, 1, 1, 4); // 4 threads, some unresolved
        let p = Palette::default();
        let (lines, _, unresolved, _) = build_content(&detail, false, false, &p, false);

        #[allow(clippy::cast_possible_truncation)]
        let total = lines.len() as u16;
        for &anchor in &unresolved {
            assert!(anchor < total, "unresolved anchor {anchor} >= total lines {total}");
        }
    }

    /// Alternating-bg ranges must cover every other top-level comment (threads
    /// + issue comments) and stay within the comments section's line range.
    #[test]
    fn alt_bg_ranges_alternate_and_stay_within_comments_section() {
        // Fixture: 3 threads + 1 issue comment = 4 top-level items.
        // Parity flips per item, starting with `alt_on = false`, so items 2
        // and 4 receive alt bg. That's 2 ranges.
        let detail = fixture_pr_detail(0, 0, 0, 3);
        let p = Palette::default();
        let (lines, anchors, _, alt_ranges) = build_content(&detail, false, true, &p, false);

        assert_eq!(alt_ranges.len(), 2, "expected 2 alt ranges for 4 items starting off, got {alt_ranges:?}");

        // Every range must fall within the comments section, which starts at
        // the last anchor. The comments section header line itself is not in
        // any range (push_alt_range starts counting after the header).
        let comments_anchor = *anchors.last().expect("comments anchor");
        #[allow(clippy::cast_possible_truncation)]
        let total = lines.len() as u16;
        for &(start, end) in &alt_ranges {
            assert!(start > comments_anchor, "range {start}..{end} starts before/at the section header {comments_anchor}");
            assert!(end <= total, "range {start}..{end} exceeds total lines {total}");
            assert!(start < end, "empty range {start}..{end}");
        }

        // Ranges must not overlap each other.
        let mut sorted = alt_ranges.clone();
        sorted.sort_by_key(|r| r.0);
        for pair in sorted.windows(2) {
            assert!(pair[0].1 <= pair[1].0, "overlapping ranges: {pair:?}");
        }
    }

    /// With only one top-level comment, no alt range is emitted (parity starts off).
    #[test]
    fn alt_bg_empty_when_single_comment() {
        let detail = fixture_pr_detail(0, 0, 0, 0); // 0 threads, only 1 issue comment
        let p = Palette::default();
        let (_, _, _, alt_ranges) = build_content(&detail, false, true, &p, false);
        assert!(alt_ranges.is_empty(), "first top-level item should not be tinted, got {alt_ranges:?}");
    }

    /// Long lines in the tinted range must be split into multiple fully-
    /// tinted lines of exactly `width` cells each. This is the core fix for
    /// the patchy-tint bug where ratatui's word-wrap left trailing cells
    /// uncoloured.
    #[test]
    fn char_wrap_tint_splits_long_lines_into_full_width_rows() {
        let bg = ratatui::style::Color::Rgb(32, 32, 45);
        // 25 chars of content, width 10 → expect three lines of exactly 10
        // chars each, the third padded with 5 spaces.
        let original = Line::from(vec![Span::styled(
            "abcdefghijklmnopqrstuvwxy".to_owned(),
            Style::default().fg(Color::Red),
        )]);
        let wrapped = char_wrap_tint(&original, bg, 10);
        assert_eq!(wrapped.len(), 3, "25 chars at width 10 → 3 rows: {wrapped:?}");
        for (i, line) in wrapped.iter().enumerate() {
            let txt: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert_eq!(txt.chars().count(), 10, "row {i} width != 10: {txt:?}");
            assert_eq!(line.style.bg, Some(bg), "row {i} missing line-level bg");
            for span in &line.spans {
                assert_eq!(span.style.bg, Some(bg), "row {i} span missing bg");
            }
        }
        // Content preserved across the split.
        let joined: String = wrapped
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(joined.starts_with("abcdefghijklmnopqrstuvwxy"), "content preserved: {joined}");
    }

    /// An empty line (e.g. a paragraph break inside a comment body) must
    /// still emit one fully-tinted row, not vanish.
    #[test]
    fn char_wrap_tint_empty_line_yields_one_padded_row() {
        let bg = ratatui::style::Color::Rgb(1, 2, 3);
        let original: Line<'static> = Line::from(vec![]);
        let wrapped = char_wrap_tint(&original, bg, 8);
        assert_eq!(wrapped.len(), 1, "empty line must still produce one tinted row");
        let txt: String = wrapped[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(txt.chars().count(), 8, "row padded to width");
        assert_eq!(wrapped[0].style.bg, Some(bg));
    }

    /// Span styles must survive the split: a bold-red span that spans the
    /// wrap boundary should remain bold-red on both resulting rows, with
    /// the tint bg applied on top.
    #[test]
    fn char_wrap_tint_preserves_span_styling_across_split() {
        let bg = ratatui::style::Color::Rgb(10, 10, 10);
        let original = Line::from(vec![Span::styled(
            "red-text-that-spans".to_owned(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )]);
        let wrapped = char_wrap_tint(&original, bg, 8);
        assert_eq!(wrapped.len(), 3, "19 chars at width 8 → 3 rows");
        // Every content-bearing span should retain the red/bold fg from the
        // original. Padding spans (end of last row) carry only the bg.
        let mut saw_red_bold = false;
        for line in &wrapped {
            for span in &line.spans {
                if span.content.contains("red")
                    || span.content.contains("text")
                    || span.content.contains("that")
                {
                    assert_eq!(span.style.fg, Some(Color::Red), "fg lost: {span:?}");
                    assert!(
                        span.style.add_modifier.contains(Modifier::BOLD),
                        "bold lost: {span:?}"
                    );
                    saw_red_bold = true;
                }
                assert_eq!(span.style.bg, Some(bg), "bg missing: {span:?}");
            }
        }
        assert!(saw_red_bold, "never saw a styled content span");
    }

    /// `tint_line` must recolour every span, right-pad to `width`, and also
    /// set `Line::style.bg` as a belt-and-suspenders fallback. This pins the
    /// contract that regressed when we tried relying on `Line::style` alone.
    #[test]
    fn tint_line_applies_bg_and_pads_row() {
        let bg = ratatui::style::Color::Rgb(32, 32, 45);
        let original = Line::from(vec![
            Span::styled("hi ", Style::default().fg(Color::Red)),
            Span::styled("there", Style::default().fg(Color::Blue)),
        ]);
        let tinted = tint_line(&original, bg, 20);
        let text: String = tinted.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.starts_with("hi there"), "text preserved: {text:?}");
        assert_eq!(text.chars().count(), 20, "row padded to 20 cells");
        for span in &tinted.spans {
            assert_eq!(span.style.bg, Some(bg), "every span carries the tint bg");
        }
        assert_eq!(tinted.style.bg, Some(bg), "line-level bg set");
    }

    /// Files-expanded flag switches from 5 to all files visible.
    #[test]
    fn files_expanded_shows_more() {
        let detail = fixture_pr_detail(0, 0, 10, 0);
        let p = Palette::default();
        let (lines_collapsed, _, _, _) = build_content(&detail, false, false, &p, false);
        let (lines_expanded, _, _, _) = build_content(&detail, true, false, &p, false);
        assert!(lines_expanded.len() > lines_collapsed.len(), "expanded should produce more lines");
    }

    // ── New tests: markdown rendering in threads ───────────────────────────────

    /// A thread comment with a rich markdown body (heading, bold, fenced code block)
    /// must produce multiple spans — not a flat single-styled plain-text line.
    #[test]
    fn thread_comment_body_renders_as_markdown() {
        let now = Utc::now();
        let p = Palette::default();
        let detail = PrDetail {
            repo: "r".to_owned(),
            number: 1,
            title: "T".to_owned(),
            url: "u".to_owned(),
            author: "a".to_owned(),
            body_markdown: String::new(),
            base_ref: "main".to_owned(),
            head_ref: "feat".to_owned(),
            is_draft: false,
            additions: 0,
            deletions: 0,
            changed_files_count: 0,
            updated_at: now,
            created_at: now,
            merged: false,
            files: vec![],
            check_runs: vec![],
            reviews: vec![],
            review_threads: vec![ReviewThread {
                path: "src/lib.rs".to_owned(),
                line: Some(10),
                is_resolved: false,
                is_outdated: false,
                comments: vec![ReviewComment {
                    author: "bob".to_owned(),
                    // Rich body: heading + bold + code block.
                    body_markdown: "# Heading\n\n**bold** text\n\n```rust\nfn f() {}\n```"
                        .to_owned(),
                    created_at: now,
                }],
            }],
            issue_comments: vec![],
        };

        let (lines, _, _, _) = build_content(&detail, false, true, &p, false);

        // Count lines whose first non-gutter span has a non-plain style (heading or code).
        // At minimum we expect a heading line (BOLD modifier) and a code-block line.
        let styled_count = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                // Heading spans carry BOLD; code spans carry a background colour.
                s.style.add_modifier.contains(Modifier::BOLD) || s.style.bg.is_some()
            })
            .count();

        assert!(
            styled_count >= 2,
            "expected >= 2 styled spans (heading + code), got {styled_count}"
        );
    }

    /// In a thread with 3 comments, only the 2nd and 3rd author lines must carry
    /// the `↳` reply prefix; the first must not.
    #[test]
    fn thread_reply_prefix_only_on_non_first_comments() {
        let now = Utc::now();
        let p = Palette::default();
        let detail = PrDetail {
            repo: "r".to_owned(),
            number: 1,
            title: "T".to_owned(),
            url: "u".to_owned(),
            author: "a".to_owned(),
            body_markdown: String::new(),
            base_ref: "main".to_owned(),
            head_ref: "feat".to_owned(),
            is_draft: false,
            additions: 0,
            deletions: 0,
            changed_files_count: 0,
            updated_at: now,
            created_at: now,
            merged: false,
            files: vec![],
            check_runs: vec![],
            reviews: vec![],
            review_threads: vec![ReviewThread {
                path: "src/lib.rs".to_owned(),
                line: Some(5),
                is_resolved: false,
                is_outdated: false,
                comments: vec![
                    ReviewComment {
                        author: "alice".to_owned(),
                        body_markdown: "First comment".to_owned(),
                        created_at: now,
                    },
                    ReviewComment {
                        author: "bob".to_owned(),
                        body_markdown: "Second comment".to_owned(),
                        created_at: now,
                    },
                    ReviewComment {
                        author: "carol".to_owned(),
                        body_markdown: "Third comment".to_owned(),
                        created_at: now,
                    },
                ],
            }],
            issue_comments: vec![],
        };

        let (lines, _, _, _) = build_content(&detail, false, true, &p, false);

        // Collect all lines that contain an author name.
        // The reply glyph ↳ (U+21B3) appears in the span immediately before the author span.
        let reply_glyph = "\u{21b3} ";
        let has_reply_prefix =
            |line: &Line<'_>| line.spans.iter().any(|s| s.content.contains(reply_glyph));

        // Find author lines by content (`@alice`, `@bob`, `@carol`).
        let alice_line = lines.iter().find(|l| line_text(l).contains("@alice"));
        let bob_line = lines.iter().find(|l| line_text(l).contains("@bob"));
        let carol_line = lines.iter().find(|l| line_text(l).contains("@carol"));

        assert!(alice_line.is_some(), "@alice line not found");
        assert!(bob_line.is_some(), "@bob line not found");
        assert!(carol_line.is_some(), "@carol line not found");

        assert!(
            !has_reply_prefix(alice_line.expect("@alice line")),
            "@alice (first comment) must NOT have reply prefix"
        );
        assert!(
            has_reply_prefix(bob_line.expect("@bob line")),
            "@bob (second comment) must have reply prefix"
        );
        assert!(
            has_reply_prefix(carol_line.expect("@carol line")),
            "@carol (third comment) must have reply prefix"
        );
    }

    /// Unresolved thread anchor must point at the thread-header line, not a body line.
    ///
    /// We verify by checking that the line at the anchor offset contains the
    /// thread glyph (`⚑`) and the path, not an author name or body text.
    #[test]
    fn unresolved_anchor_points_at_thread_header() {
        let now = Utc::now();
        let p = Palette::default();
        // Single unresolved thread, no other sections to clutter offsets.
        let detail = PrDetail {
            repo: "r".to_owned(),
            number: 1,
            title: "T".to_owned(),
            url: "u".to_owned(),
            author: "a".to_owned(),
            body_markdown: String::new(),
            base_ref: "main".to_owned(),
            head_ref: "feat".to_owned(),
            is_draft: false,
            additions: 0,
            deletions: 0,
            changed_files_count: 0,
            updated_at: now,
            created_at: now,
            merged: false,
            files: vec![],
            check_runs: vec![],
            reviews: vec![],
            review_threads: vec![ReviewThread {
                path: "src/lib.rs".to_owned(),
                line: Some(42),
                is_resolved: false,
                is_outdated: false,
                comments: vec![ReviewComment {
                    author: "bob".to_owned(),
                    body_markdown: "Needs refactor.".to_owned(),
                    created_at: now,
                }],
            }],
            issue_comments: vec![],
        };

        let (lines, _, unresolved, _) = build_content(&detail, false, true, &p, false);

        assert_eq!(unresolved.len(), 1, "expected exactly 1 unresolved anchor");
        let anchor = unresolved[0] as usize;
        assert!(anchor < lines.len(), "anchor out of bounds");

        let header_text = line_text(&lines[anchor]);
        // The thread header contains the path and the unresolved glyph ⚑.
        assert!(
            header_text.contains("src/lib.rs"),
            "anchor line should contain file path, got: {header_text:?}"
        );
        assert!(
            header_text.contains('\u{2691}'), // ⚑
            "anchor line should contain ⚑ glyph, got: {header_text:?}"
        );
    }

    /// Replies inside a review thread must visually stand out from the
    /// opener. We pin two parts of the contract:
    ///   1. The reply's `↳` glyph and @handle render in `accent_alt` (not
    ///      the regular foreground the opener uses).
    ///   2. The reply body's gutter rail is tinted with `accent_alt`, so
    ///      the vertical `│` that flanks every reply line stays visibly
    ///      coloured even through long bodies.
    #[test]
    fn replies_render_in_accent_alt() {
        let now = Utc::now();
        let p = Palette::default();
        let detail = PrDetail {
            repo: "r".to_owned(),
            number: 1,
            title: "T".to_owned(),
            url: "u".to_owned(),
            author: "a".to_owned(),
            body_markdown: String::new(),
            base_ref: "main".to_owned(),
            head_ref: "feat".to_owned(),
            is_draft: false,
            additions: 0,
            deletions: 0,
            changed_files_count: 0,
            updated_at: now,
            created_at: now,
            merged: false,
            files: vec![],
            check_runs: vec![],
            reviews: vec![],
            review_threads: vec![ReviewThread {
                path: "src/lib.rs".to_owned(),
                line: Some(1),
                is_resolved: false,
                is_outdated: false,
                comments: vec![
                    ReviewComment {
                        author: "opener".to_owned(),
                        body_markdown: "Opening thought.".to_owned(),
                        created_at: now,
                    },
                    ReviewComment {
                        author: "replier".to_owned(),
                        body_markdown: "Counter-point.".to_owned(),
                        created_at: now,
                    },
                ],
            }],
            issue_comments: vec![],
        };

        let (lines, _, _, _) = build_content(&detail, false, true, &p, false);

        // The reply @handle span must be in accent_alt.
        let reply_author = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.as_ref() == "@replier")
            .expect("reply author span");
        assert_eq!(
            reply_author.style.fg,
            Some(p.accent_alt),
            "reply @handle must be accent_alt to stand out from opener"
        );

        // The opener @handle must still be foreground (contract preserved).
        let opener_author = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.as_ref() == "@opener")
            .expect("opener author span");
        assert_eq!(
            opener_author.style.fg,
            Some(p.foreground),
            "opener @handle must stay in plain foreground"
        );

        // Somewhere in the reply body there must be a gutter span tinted
        // accent_alt — that's what visually frames the reply block.
        let reply_gutter_count = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                s.content.as_ref().contains('\u{2502}') && s.style.fg == Some(p.accent_alt)
            })
            .count();
        assert!(
            reply_gutter_count > 0,
            "expected at least one accent_alt gutter rail for the reply"
        );
    }

    /// A comment body exceeding 6 rendered lines when collapsed must show the
    /// `[m] expand` hint and not show all body lines.
    #[test]
    fn collapsed_long_comment_shows_expand_hint() {
        let now = Utc::now();
        let p = Palette::default();
        // 10 paragraphs → render_markdown produces >> 6 lines.
        let long_body = (0..10).map(|i| format!("Paragraph {i}.")).collect::<Vec<_>>().join("\n\n");

        let detail = PrDetail {
            repo: "r".to_owned(),
            number: 1,
            title: "T".to_owned(),
            url: "u".to_owned(),
            author: "a".to_owned(),
            body_markdown: String::new(),
            base_ref: "main".to_owned(),
            head_ref: "feat".to_owned(),
            is_draft: false,
            additions: 0,
            deletions: 0,
            changed_files_count: 0,
            updated_at: now,
            created_at: now,
            merged: false,
            files: vec![],
            check_runs: vec![],
            reviews: vec![],
            review_threads: vec![ReviewThread {
                path: "src/lib.rs".to_owned(),
                line: Some(1),
                is_resolved: false,
                is_outdated: false,
                comments: vec![ReviewComment {
                    author: "alice".to_owned(),
                    body_markdown: long_body,
                    created_at: now,
                }],
            }],
            issue_comments: vec![],
        };

        // collapsed = false (comments_expanded = false)
        let (lines, _, _, _) = build_content(&detail, false, false, &p, false);

        let has_expand_hint = lines.iter().any(|l| line_text(l).contains("[m] expand"));

        assert!(has_expand_hint, "collapsed long comment must show [m] expand hint");
    }

    /// Issue comments must render markdown (bold/inline-code) rather than plain text.
    #[test]
    fn issue_comments_render_markdown_styles() {
        let now = Utc::now();
        let p = Palette::default();
        let detail = PrDetail {
            repo: "r".to_owned(),
            number: 1,
            title: "T".to_owned(),
            url: "u".to_owned(),
            author: "a".to_owned(),
            body_markdown: String::new(),
            base_ref: "main".to_owned(),
            head_ref: "feat".to_owned(),
            is_draft: false,
            additions: 0,
            deletions: 0,
            changed_files_count: 0,
            updated_at: now,
            created_at: now,
            merged: false,
            files: vec![],
            check_runs: vec![],
            reviews: vec![],
            review_threads: vec![],
            // Issue comment with bold and inline-code so we can detect styled spans.
            issue_comments: vec![IssueComment {
                author: "dave".to_owned(),
                body_markdown: "**important** and `code_snippet`".to_owned(),
                created_at: now,
            }],
        };

        let (lines, _, _, _) = build_content(&detail, false, true, &p, false);

        // Bold span for "important".
        let has_bold = lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
            s.content.contains("important") && s.style.add_modifier.contains(Modifier::BOLD)
        });

        // Inline-code span with code_bg background.
        let has_code = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.content.contains("code_snippet") && s.style.bg == Some(p.code_bg));

        assert!(has_bold, "issue comment body must render **bold** with BOLD modifier");
        assert!(has_code, "issue comment body must render `code_snippet` with code_bg");
    }
}
