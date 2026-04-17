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
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

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
    let rule = "\u{2500}".repeat(4); // ────
    Line::from(Span::styled(
        format!("{rule} {label} {rule}"),
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
    ))
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
/// Each `Line` from `render_markdown` gets a leading gutter span
/// (`palette.block_quote_border`) prepended.
///
/// When the incoming line's first span has a background color (typical for
/// syntect-highlighted code-block lines), the gutter span inherits that
/// background so the code-block's colored rail extends cleanly through the
/// gutter column instead of breaking to the terminal default.
fn gutter_lines(
    md_lines: Vec<Line<'static>>,
    p: &crate::theme::Palette,
    ascii: bool,
) -> Vec<Line<'static>> {
    md_lines
        .into_iter()
        .map(|mut line| {
            let inherited_bg = line.spans.first().and_then(|s| s.style.bg);
            let mut style = Style::default().fg(p.block_quote_border);
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
) -> (Vec<Line<'static>>, Vec<u16>) {
    let gutter = thread_gutter(ascii);
    let reply_glyph = if ascii { "> " } else { "\u{21b3} " };
    let unresolved_count = detail.review_threads.iter().filter(|t| !t.is_resolved).count();
    let total_threads = detail.review_threads.len();
    let total_comments = detail.issue_comments.len();

    let mut lines = Vec::new();
    // Track relative Y of each unresolved thread header (within this block, after the section
    // header). These are shifted by +1 when the section header is prepended.
    let mut unresolved_offsets: Vec<u16> = Vec::new();

    // Sort threads: unresolved first.
    let mut threads: Vec<&ReviewThread> = detail.review_threads.iter().collect();
    threads.sort_by_key(|t| t.is_resolved);

    // When collapsed, allow up to 10 total items (threads + issue comments).
    let max_items = if expanded { usize::MAX } else { 10 };
    let mut items_shown = 0;

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

        // Thread header: `  ⚑ src/foo.rs:42  ·  2 comments  ·  unresolved`
        lines.push(thread_header_line(thread, p, ascii));

        for (idx, comment) in thread.comments.iter().enumerate() {
            let age = humanize_delta(&comment.created_at);

            // The first comment is the thread opener; subsequent ones are replies.
            // We signal replies with `↳ ` in palette.dim on the author line,
            // and additionally step-in reply BODY lines by two extra spaces so
            // a long reply doesn't blur into the previous comment's body.
            let author_line = if idx == 0 {
                Line::from(vec![
                    Span::styled(gutter, Style::default().fg(p.block_quote_border)),
                    Span::styled(
                        format!("@{}", comment.author),
                        Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("  {age}"), Style::default().fg(p.dim)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(gutter, Style::default().fg(p.block_quote_border)),
                    Span::styled(reply_glyph, Style::default().fg(p.dim)),
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
            // comment's body (which sits flush against the gutter).
            let body_lines = if idx == 0 {
                gutter_lines(visible_rendered, p, ascii)
            } else {
                gutter_lines(indent_lines(visible_rendered, "  "), p, ascii)
            };
            lines.extend(body_lines);

            if truncated {
                lines.push(Line::from(vec![
                    Span::styled(gutter, Style::default().fg(p.block_quote_border)),
                    Span::styled("[m] expand", Style::default().fg(p.dim)),
                ]));
            }

            // Blank gutter line between comments within the same thread so code
            // blocks don't blur into each other, but the `│` rail shows they're
            // still part of the same conversation.
            if idx + 1 < thread.comments.len() {
                lines.push(Line::from(vec![Span::styled(
                    gutter,
                    Style::default().fg(p.block_quote_border),
                )]));
            }
        }

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

    (all_lines, shifted_offsets)
}

// ── Top-level content builder ─────────────────────────────────────────────────

/// Build all content lines for the PR detail view.
///
/// Returns `(lines, section_anchors, unresolved_thread_anchors)` where anchors
/// are absolute Y offsets within the content.
///
/// `unresolved_thread_anchors` always point at the thread-header line for each
/// unresolved thread so `n`/`N` navigation in the key handler works correctly.
pub fn build_content(
    detail: &PrDetail,
    files_expanded: bool,
    comments_expanded: bool,
    p: &crate::theme::Palette,
    ascii: bool,
) -> (Vec<Line<'static>>, SectionAnchors, Vec<u16>) {
    let mut all_lines: Vec<Line<'static>> = Vec::new();
    let mut section_anchors: SectionAnchors = Vec::new();
    let mut unresolved_anchors: Vec<u16> = Vec::new();

    // ── Banner ────────────────────────────────────────────────────────────────
    #[allow(clippy::cast_possible_truncation)]
    let banner_anchor = all_lines.len() as u16;
    section_anchors.push(banner_anchor);
    if let Some(banner) = banner_line(detail, p) {
        all_lines.push(banner);
    } else {
        all_lines.push(Line::from("")); // placeholder keeps anchor math stable
    }
    all_lines.push(Line::from(""));

    // ── Title ─────────────────────────────────────────────────────────────────
    #[allow(clippy::cast_possible_truncation)]
    let title_anchor = all_lines.len() as u16;
    section_anchors.push(title_anchor);
    all_lines.push(Line::from(Span::styled(
        detail.title.clone(),
        Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
    )));
    all_lines.push(Line::from(""));

    // ── Meta ──────────────────────────────────────────────────────────────────
    let age = humanize_delta(&detail.created_at);
    let meta = format!(
        "@{}  opened {}  \u{00B7}  +{} \u{2212}{}  \u{00B7}  {} files  \u{00B7}  {} comments",
        detail.author,
        age,
        detail.additions,
        detail.deletions,
        detail.changed_files_count,
        detail.issue_comments.len(),
    );
    all_lines.push(Line::from(Span::styled(meta, Style::default().fg(p.dim))));
    all_lines.push(Line::from(""));

    // ── Body (rendered Markdown) ───────────────────────────────────────────────
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
        let (comment_lines, thread_offsets) = comments_lines(detail, comments_expanded, p, ascii);
        // Convert thread relative offsets to absolute Y offsets.
        for offset in thread_offsets {
            unresolved_anchors.push(comments_anchor + offset);
        }
        all_lines.extend(comment_lines);
    }

    (all_lines, section_anchors, unresolved_anchors)
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

    let (content_lines, _section_anchors, _unresolved_anchors) = build_content(
        detail,
        app.pr_detail_files_expanded,
        app.pr_detail_comments_expanded,
        p,
        app.config.show_ascii_glyphs,
    );

    let scroll = app.pr_detail_scroll;

    let widget = Paragraph::new(content_lines)
        .style(Style::default().bg(p.background).fg(p.foreground))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    f.render_widget(widget, area);
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
        let (_, anchors, _) = build_content(&detail, false, false, &p, false);

        // We have: banner, title, checks, reviews, files, comments = 6 anchors.
        assert!(!anchors.is_empty(), "anchors should not be empty");

        // Each anchor must be >= the previous (monotone non-decreasing).
        for window in anchors.windows(2) {
            assert!(window[1] >= window[0], "anchors not monotone: {anchors:?}");
        }
    }

    /// No checks/reviews/files: anchor list should only have banner + title.
    #[test]
    fn section_anchors_count_matches_content() {
        let detail = fixture_pr_detail(2, 1, 3, 1);
        let p = Palette::default();
        let (_, anchors, _) = build_content(&detail, false, false, &p, false);
        // banner + title + checks + reviews + files + comments = 6
        assert_eq!(anchors.len(), 6, "expected 6 anchors for full fixture, got {}", anchors.len());
    }

    /// Unresolved thread anchors must be a subset of the total anchor range.
    #[test]
    fn unresolved_anchors_within_total_lines() {
        let detail = fixture_pr_detail(1, 1, 1, 4); // 4 threads, some unresolved
        let p = Palette::default();
        let (lines, _, unresolved) = build_content(&detail, false, false, &p, false);

        #[allow(clippy::cast_possible_truncation)]
        let total = lines.len() as u16;
        for &anchor in &unresolved {
            assert!(anchor < total, "unresolved anchor {anchor} >= total lines {total}");
        }
    }

    /// Files-expanded flag switches from 5 to all files visible.
    #[test]
    fn files_expanded_shows_more() {
        let detail = fixture_pr_detail(0, 0, 10, 0);
        let p = Palette::default();
        let (lines_collapsed, _, _) = build_content(&detail, false, false, &p, false);
        let (lines_expanded, _, _) = build_content(&detail, true, false, &p, false);
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

        let (lines, _, _) = build_content(&detail, false, true, &p, false);

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

        let (lines, _, _) = build_content(&detail, false, true, &p, false);

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

        let (lines, _, unresolved) = build_content(&detail, false, true, &p, false);

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
        let (lines, _, _) = build_content(&detail, false, false, &p, false);

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

        let (lines, _, _) = build_content(&detail, false, true, &p, false);

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
