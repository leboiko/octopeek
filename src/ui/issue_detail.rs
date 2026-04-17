//! Issue detail panel — renders title, meta, body, and comments.
//!
//! Layout (vertically scrollable):
//! 1. Title
//! 2. Meta line (author, age, comments count, labels)
//! 3. Body (rendered Markdown)
//! 4. COMMENTS section
//!
//! ## Comment rendering contract
//!
//! Each issue comment is rendered as:
//! - `@handle` bold (`palette.foreground`) + `  <age>` dim
//! - Body: full GFM-rendered markdown, indented by `"  "` (two spaces). No `│`
//!   gutter — these are flat top-level comments with nothing to tether.
//! - Blank line between comments.
//!
//! When `comments_expanded == false`, each comment body is capped at 6 rendered
//! lines and a `[m] expand` hint line is appended.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, Wrap},
};

use crate::app::App;
use crate::github::detail::IssueDetail;
use crate::ui::markdown::render_markdown;
use crate::ui::util::humanize_delta;

// ── Section header helper ─────────────────────────────────────────────────────

fn section_header(label: &str, p: &crate::theme::Palette) -> Line<'static> {
    let rule = "\u{2501}".repeat(3); // ━━━
    Line::from(vec![
        Span::styled(format!("{rule} "), Style::default().fg(p.accent)),
        Span::styled(
            label.to_owned(),
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        ),
    ])
}

// ── Content builder ───────────────────────────────────────────────────────────

/// Build all content lines for the issue detail view.
///
/// Each comment body is rendered via [`render_markdown`] so inline styles,
/// code blocks, and headings display correctly. When `comments_expanded` is
/// `false`, bodies exceeding 6 rendered lines are capped and a `[m] expand`
/// hint is shown.
///
/// # Returns
///
/// `(lines, section_anchors)` where `section_anchors[0]` is always the title
/// (Y = 0) and `section_anchors[1]` (when present) is the COMMENTS header.
pub fn build_content(
    detail: &IssueDetail,
    comments_expanded: bool,
    p: &crate::theme::Palette,
    _ascii: bool,
) -> (Vec<Line<'static>>, Vec<u16>) {
    // `_ascii` is accepted for signature parity with `pr_detail::build_content`
    // even though the issue detail does not currently use glyphs that need a
    // fallback (the only Unicode characters are the `·` middle dot, which is
    // ubiquitous, and header dashes). Preserving the parameter makes future
    // glyph additions a one-line change.
    let mut all_lines: Vec<Line<'static>> = Vec::new();
    let mut section_anchors: Vec<u16> = Vec::new();

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
    let labels_str = if detail.labels.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = detail.labels.iter().map(|l| l.name.as_str()).collect();
        format!("  \u{00B7}  labels: {}", names.join(", "))
    };
    let meta = format!(
        "@{}  opened {}  \u{00B7}  {} comments{}",
        detail.author,
        age,
        detail.comments.len(),
        labels_str,
    );
    all_lines.push(Line::from(Span::styled(meta, Style::default().fg(p.dim))));
    all_lines.push(Line::from(""));

    // ── Body (rendered Markdown) ───────────────────────────────────────────────
    if !detail.body_markdown.is_empty() {
        let body_lines = render_markdown(&detail.body_markdown, p);
        all_lines.extend(body_lines);
        all_lines.push(Line::from(""));
    }

    // ── COMMENTS ──────────────────────────────────────────────────────────────
    if !detail.comments.is_empty() {
        #[allow(clippy::cast_possible_truncation)]
        let comments_anchor = all_lines.len() as u16;
        section_anchors.push(comments_anchor);
        all_lines.push(section_header(&format!("COMMENTS ({})", detail.comments.len()), p));

        for comment in &detail.comments {
            let age = humanize_delta(&comment.created_at);

            // Author header: `@handle` bold, then `  <age>` dim.
            all_lines.push(Line::from(vec![
                Span::styled(
                    format!("@{}", comment.author),
                    Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {age}"), Style::default().fg(p.dim)),
            ]));

            // Body rendered as full GFM markdown, indented by two spaces.
            // When collapsed, cap at 6 rendered lines and show expand hint.
            let body = comment.body_markdown.trim();
            let rendered = render_markdown(body, p);
            let total_rendered = rendered.len();

            let (visible_rendered, truncated) = if !comments_expanded && total_rendered > 6 {
                (rendered.into_iter().take(6).collect::<Vec<_>>(), true)
            } else {
                (rendered, false)
            };

            // Prepend a `"  "` indent to each body line (no gutter — flat comments).
            for mut line in visible_rendered {
                line.spans.insert(0, Span::raw("  "));
                all_lines.push(line);
            }

            if truncated {
                all_lines
                    .push(Line::from(Span::styled("  [m] expand", Style::default().fg(p.dim))));
            }

            all_lines.push(Line::from("")); // blank separator between comments
        }
    }

    (all_lines, section_anchors)
}

// ── draw (public entry point) ─────────────────────────────────────────────────

/// Render the issue detail panel into `area`.
///
/// Handles three states:
/// - Fetching (no detail yet): centered spinner text.
/// - Error (fetch failed): error panel with retry hint.
/// - Loaded: full scrollable detail layout.
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let p = &app.palette;

    // ── A. Loading ─────────────────────────────────────────────────────────────
    if app.detail_fetching && app.issue_detail.is_none() {
        let widget = Paragraph::new(Line::from(Span::styled(
            "Fetching issue\u{2026}",
            Style::default().fg(p.dim),
        )))
        .block(Block::default().style(Style::default().bg(p.background)))
        .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(widget, area);
        return;
    }

    // ── B. Error ───────────────────────────────────────────────────────────────
    if let Some(err) = &app.detail_error
        && app.issue_detail.is_none()
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
    let Some(detail) = &app.issue_detail else {
        return;
    };

    // `pr_detail_comments_expanded` and `pr_detail_scroll` are intentionally
    // reused for issue detail: both views are mutually exclusive (only one
    // detail kind is active at a time), and sharing the fields means the `m`
    // expand key and the scroll offset behave the same regardless of which
    // kind of detail is open. A future refactor that adds parallel
    // `issue_detail_*` fields would need to re-plumb those keybindings.
    let (content_lines, _section_anchors) =
        build_content(detail, app.pr_detail_comments_expanded, p, app.config.show_ascii_glyphs);

    // Wrap the paragraph in a padded block — see pr_detail::draw for rationale.
    let block = Block::default()
        .style(Style::default().bg(p.background).fg(p.foreground))
        .padding(Padding::new(2, 2, 1, 0));
    let inner = block.inner(area);
    app.pr_detail_viewport.set(inner);

    let scroll = app.pr_detail_scroll;

    let widget = if app.copy_mode.active {
        let overlaid = crate::ui::copy_mode::apply_overlay(&content_lines, &app.copy_mode, p);
        Paragraph::new(overlaid)
            .block(block)
            .style(Style::default().bg(p.background).fg(p.foreground))
            .scroll((scroll, app.copy_mode.h_scroll))
    } else {
        Paragraph::new(content_lines)
            .block(block)
            .style(Style::default().bg(p.background).fg(p.foreground))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
    };

    f.render_widget(widget, area);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::github::detail::IssueComment;
    use crate::github::types::Label;
    use crate::theme::Palette;
    use chrono::Utc;

    fn fixture_issue_detail(num_comments: usize) -> IssueDetail {
        let now = Utc::now();
        let comments = (0..num_comments)
            .map(|i| IssueComment {
                author: format!("user-{i}"),
                body_markdown: format!("Comment body {i}"),
                created_at: now,
            })
            .collect();

        IssueDetail {
            repo: "owner/repo".to_owned(),
            number: 7,
            title: "Test Issue".to_owned(),
            url: "https://github.com/owner/repo/issues/7".to_owned(),
            author: "dave".to_owned(),
            body_markdown: "Reproducible with an empty config.".to_owned(),
            state: "OPEN".to_owned(),
            updated_at: now,
            created_at: now,
            labels: vec![Label { name: "bug".to_owned(), color: "ee0701".to_owned() }],
            assignees: vec!["alice".to_owned()],
            comments,
        }
    }

    /// Issue detail anchors: always starts with title anchor at 0.
    #[test]
    fn issue_detail_anchors_start_at_zero() {
        let detail = fixture_issue_detail(3);
        let p = Palette::default();
        let (_, anchors) = build_content(&detail, false, &p, false);
        assert!(!anchors.is_empty(), "should have at least one anchor");
        assert_eq!(anchors[0], 0, "title anchor should be at 0");
    }

    /// Anchors must be monotonically non-decreasing.
    #[test]
    fn issue_detail_anchors_monotone() {
        let detail = fixture_issue_detail(5);
        let p = Palette::default();
        let (_, anchors) = build_content(&detail, false, &p, false);
        for window in anchors.windows(2) {
            assert!(window[1] >= window[0], "anchors not monotone: {anchors:?}");
        }
    }

    /// With no comments the anchor list has only the title anchor.
    #[test]
    fn issue_detail_no_comments_one_anchor() {
        let detail = fixture_issue_detail(0);
        let p = Palette::default();
        let (_, anchors) = build_content(&detail, false, &p, false);
        assert_eq!(anchors.len(), 1, "no comments => only title anchor");
    }

    /// Issue comment bodies render markdown: bold and inline-code produce styled spans.
    #[test]
    fn issue_comment_body_renders_markdown_styles() {
        let now = Utc::now();
        let p = Palette::default();
        let detail = IssueDetail {
            repo: "owner/repo".to_owned(),
            number: 1,
            title: "Issue".to_owned(),
            url: "u".to_owned(),
            author: "dave".to_owned(),
            body_markdown: String::new(),
            state: "OPEN".to_owned(),
            updated_at: now,
            created_at: now,
            labels: vec![],
            assignees: vec![],
            comments: vec![IssueComment {
                author: "eve".to_owned(),
                // Bold + inline code in the body.
                body_markdown: "**critical** and `fix_it()`".to_owned(),
                created_at: now,
            }],
        };

        let (lines, _) = build_content(&detail, true, &p, false);

        // Bold span for "critical".
        let has_bold = lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
            s.content.contains("critical") && s.style.add_modifier.contains(Modifier::BOLD)
        });

        // Inline-code span with code_bg background.
        let has_code = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.content.contains("fix_it()") && s.style.bg == Some(p.code_bg));

        assert!(has_bold, "issue comment **bold** must produce BOLD modifier span");
        assert!(has_code, "issue comment `code` must produce code_bg span");
    }

    /// A comment body with > 6 rendered lines when collapsed must show `[m] expand`.
    #[test]
    fn issue_comment_collapsed_shows_expand_hint() {
        let now = Utc::now();
        let p = Palette::default();
        let long_body = (0..10).map(|i| format!("Para {i}.")).collect::<Vec<_>>().join("\n\n");

        let detail = IssueDetail {
            repo: "owner/repo".to_owned(),
            number: 1,
            title: "Issue".to_owned(),
            url: "u".to_owned(),
            author: "dave".to_owned(),
            body_markdown: String::new(),
            state: "OPEN".to_owned(),
            updated_at: now,
            created_at: now,
            labels: vec![],
            assignees: vec![],
            comments: vec![IssueComment {
                author: "frank".to_owned(),
                body_markdown: long_body,
                created_at: now,
            }],
        };

        let (lines, _) = build_content(&detail, false, &p, false);

        let has_hint =
            lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains("[m] expand")));

        assert!(has_hint, "collapsed long issue comment must show [m] expand hint");
    }
}
