//! Issue detail panel — renders title, meta, body, and comments.
//!
//! Layout (vertically scrollable):
//! 1. Title
//! 2. Meta line (author, age, comments count, labels)
//! 3. Body (rendered Markdown)
//! 4. COMMENTS section

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

use crate::app::App;
use crate::github::detail::IssueDetail;
use crate::ui::markdown::render_markdown;
use crate::ui::util::humanize_delta;

// ── Section header helper ─────────────────────────────────────────────────────

fn section_header(label: &str, p: &crate::theme::Palette) -> Line<'static> {
    let rule = "\u{2500}".repeat(4);
    Line::from(Span::styled(
        format!("{rule} {label} {rule}"),
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
    ))
}

// ── Content builder ───────────────────────────────────────────────────────────

/// Build all content lines for the issue detail view.
///
/// Returns `(lines, section_anchors)`.
pub fn build_content(
    detail: &IssueDetail,
    p: &crate::theme::Palette,
) -> (Vec<Line<'static>>, Vec<u16>) {
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
            all_lines.push(Line::from(vec![
                Span::styled(
                    format!("@{}", comment.author),
                    Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {age}"), Style::default().fg(p.dim)),
            ]));
            // Render body as plain text (first 5 lines for compact view).
            for body_line in comment.body_markdown.trim().lines().take(5) {
                all_lines.push(Line::from(Span::styled(
                    format!("  {body_line}"),
                    Style::default().fg(p.foreground),
                )));
            }
            all_lines.push(Line::from(""));
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

    let (content_lines, _section_anchors) = build_content(detail, p);

    let scroll = app.pr_detail_scroll; // reuse same offset field for simplicity

    let widget = Paragraph::new(content_lines)
        .style(Style::default().bg(p.background).fg(p.foreground))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

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
        let (_, anchors) = build_content(&detail, &p);
        assert!(!anchors.is_empty(), "should have at least one anchor");
        assert_eq!(anchors[0], 0, "title anchor should be at 0");
    }

    /// Anchors must be monotonically non-decreasing.
    #[test]
    fn issue_detail_anchors_monotone() {
        let detail = fixture_issue_detail(5);
        let p = Palette::default();
        let (_, anchors) = build_content(&detail, &p);
        for window in anchors.windows(2) {
            assert!(window[1] >= window[0], "anchors not monotone: {anchors:?}");
        }
    }

    /// With no comments the anchor list has only the title anchor.
    #[test]
    fn issue_detail_no_comments_one_anchor() {
        let detail = fixture_issue_detail(0);
        let p = Palette::default();
        let (_, anchors) = build_content(&detail, &p);
        assert_eq!(anchors.len(), 1, "no comments => only title anchor");
    }
}
