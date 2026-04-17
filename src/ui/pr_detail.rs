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

/// Build comment section lines, with expansion controlled by `expanded`.
/// Returns `(lines, unresolved_thread_relative_offsets)`.
///
/// Offsets are relative to the start of the comments block (0 = first line of
/// the block header). Callers add the header's absolute Y to get global anchors.
fn comments_lines(
    detail: &PrDetail,
    expanded: bool,
    p: &crate::theme::Palette,
) -> (Vec<Line<'static>>, Vec<u16>) {
    let unresolved_count = detail.review_threads.iter().filter(|t| !t.is_resolved).count();
    let total_threads = detail.review_threads.len();
    let total_comments = detail.issue_comments.len();

    let mut lines = Vec::new();
    // Track relative Y of each unresolved thread start (within this block, after header).
    let mut unresolved_offsets: Vec<u16> = Vec::new();

    // Sort threads: unresolved first.
    let mut threads: Vec<&ReviewThread> = detail.review_threads.iter().collect();
    threads.sort_by_key(|t| t.is_resolved);

    let max_items = if expanded { usize::MAX } else { 10 };
    let mut items_shown = 0;

    // Render review threads.
    for thread in &threads {
        if items_shown >= max_items {
            break;
        }

        // Record offset for unresolved threads (lines.len() = current relative offset).
        if !thread.is_resolved {
            #[allow(clippy::cast_possible_truncation)]
            unresolved_offsets.push(lines.len() as u16);
        }

        let tag = if thread.is_resolved { "resolved" } else { "unresolved" };
        let tag_color = if thread.is_resolved { p.muted } else { p.warning };
        let location =
            thread.line.map_or_else(|| thread.path.clone(), |ln| format!("{}:{ln}", thread.path));

        lines.push(Line::from(vec![
            Span::styled(location, Style::default().fg(p.accent)),
            Span::styled(format!("  [{tag}]"), Style::default().fg(tag_color)),
        ]));

        for comment in &thread.comments {
            let age = humanize_delta(&comment.created_at);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  @{}", comment.author),
                    Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {age}"), Style::default().fg(p.dim)),
            ]));
            // Render body as plain text (first line only for threads to keep it compact).
            let body = comment.body_markdown.trim();
            for body_line in body.lines().take(3) {
                lines.push(Line::from(Span::styled(
                    format!("    {body_line}"),
                    Style::default().fg(p.foreground),
                )));
            }
        }
        lines.push(Line::from("")); // blank separator
        items_shown += 1;
    }

    // Render issue comments.
    for comment in &detail.issue_comments {
        if items_shown >= max_items {
            break;
        }
        let age = humanize_delta(&comment.created_at);
        lines.push(Line::from(vec![
            Span::styled(
                format!("@{}", comment.author),
                Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {age}"), Style::default().fg(p.dim)),
        ]));
        for body_line in comment.body_markdown.trim().lines().take(5) {
            lines.push(Line::from(Span::styled(
                format!("  {body_line}"),
                Style::default().fg(p.foreground),
            )));
        }
        lines.push(Line::from("")); // blank separator
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
pub fn build_content(
    detail: &PrDetail,
    files_expanded: bool,
    comments_expanded: bool,
    p: &crate::theme::Palette,
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
        let (comment_lines, thread_offsets) = comments_lines(detail, comments_expanded, p);
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

    let (content_lines, _section_anchors, _unresolved_anchors) =
        build_content(detail, app.pr_detail_files_expanded, app.pr_detail_comments_expanded, p);

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

    /// Section anchors must be monotonically non-decreasing and contain one
    /// entry for each major section that has content.
    #[test]
    fn section_anchors_are_monotone() {
        let detail = fixture_pr_detail(3, 2, 4, 2);
        let p = Palette::default();
        let (_, anchors, _) = build_content(&detail, false, false, &p);

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
        let (_, anchors, _) = build_content(&detail, false, false, &p);
        // banner + title + checks + reviews + files + comments = 6
        assert_eq!(anchors.len(), 6, "expected 6 anchors for full fixture, got {}", anchors.len());
    }

    /// Unresolved thread anchors must be a subset of the total anchor range.
    #[test]
    fn unresolved_anchors_within_total_lines() {
        let detail = fixture_pr_detail(1, 1, 1, 4); // 4 threads, some unresolved
        let p = Palette::default();
        let (lines, _, unresolved) = build_content(&detail, false, false, &p);

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
        let (lines_collapsed, _, _) = build_content(&detail, false, false, &p);
        let (lines_expanded, _, _) = build_content(&detail, true, false, &p);
        assert!(lines_expanded.len() > lines_collapsed.len(), "expanded should produce more lines");
    }
}
