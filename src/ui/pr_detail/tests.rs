//! Unit tests for the `pr_detail` module.
//!
//! Moved wholesale from the bottom of the original monolithic `pr_detail.rs`.

// See the same note in `src/app/tests.rs`: tests lean on `.unwrap()` /
// `.expect()` for assertion-site failures; production code keeps the
// lints set in Cargo.toml.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::RefCell;
use std::collections::HashSet;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

use crate::github::detail::{
    DetailedCheck, DetailedReview, FileChange, FileChangeKind, IssueComment, PrCommit, PrDetail,
    ReviewComment, ReviewThread,
};
use crate::github::types::ReviewState;
use crate::theme::Palette;
use chrono::Utc;

use super::DetailSection;
use super::comments::comments_lines;
use super::header::{build_header, char_wrap_tint, tint_line};
use super::sections::build_section;

/// Convenience: empty expanded-threads set for tests that don't exercise expansion.
fn no_expanded() -> HashSet<(String, u32)> {
    HashSet::new()
}

/// Convenience: empty diff cursor for tests that don't exercise cursor tracking.
fn no_cursor() -> RefCell<Option<(String, u32)>> {
    RefCell::new(None)
}

/// Build a fixture `PrDetail` with a configurable number of checks, reviews, files, and threads.
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
            state: if i % 2 == 0 { ReviewState::Approved } else { ReviewState::ChangesRequested },
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
            change_kind: if i % 2 == 0 { FileChangeKind::Modified } else { FileChangeKind::Added },
            patch: None,
        })
        .collect();

    let review_threads = (0..num_threads)
        .map(|i| ReviewThread {
            node_id: "THREAD_node".to_owned(),
            path: format!("src/file-{i}.rs"),
            #[allow(clippy::cast_possible_truncation)]
            line: Some((i as u32 + 1) * 5),
            start_line: None,
            is_resolved: i % 3 == 0,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![ReviewComment {
                node_id: "COMMENT_node".to_owned(),
                author: format!("user-{i}"),
                body_markdown: format!("Comment {i}"),
                created_at: now,
                diff_hunk: None,
                original_commit_id: None,
            }],
        })
        .collect();

    PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "owner/repo".to_owned(),
        number: 1,
        title: "Test PR".to_owned(),
        url: "https://github.com/owner/repo/pull/1".to_owned(),
        author: "alice".to_owned(),
        body_markdown: "## Summary\n\nThis is a test PR.".to_owned(),
        base_ref: "main".to_owned(),
        head_ref: "feat/test".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
            node_id: "COMMENT_node".to_owned(),
            author: "carol".to_owned(),
            body_markdown: "Nice work!".to_owned(),
            created_at: now,
        }],
        commits: vec![],
    }
}

/// Helper: concatenate all span text in a line.
fn line_text(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

#[test]
fn build_section_non_empty_sections_have_lines() {
    let detail = fixture_pr_detail(2, 1, 3, 1);
    let p = Palette::default();

    let (desc, _) = build_section(
        DetailSection::Description,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    assert!(!desc.is_empty(), "Description must produce lines when body is non-empty");

    let (checks, _) = build_section(
        DetailSection::Checks,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    assert!(!checks.is_empty(), "Checks must produce lines when check_runs is non-empty");

    let (reviews, _) = build_section(
        DetailSection::Reviews,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    assert!(!reviews.is_empty(), "Reviews must produce lines when reviews is non-empty");

    let (files, _) = build_section(
        DetailSection::Files,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    assert!(!files.is_empty(), "Files must produce lines when files is non-empty");

    let (comments, _) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    assert!(!comments.is_empty(), "Comments must produce lines when threads are present");
}

#[test]
fn build_section_empty_sections_have_no_lines() {
    let detail = fixture_pr_detail(0, 0, 0, 0); // only issue comment, no threads
    let p = Palette::default();

    let (checks, _) = build_section(
        DetailSection::Checks,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    assert!(checks.is_empty(), "Checks must be empty when no check_runs");

    let (reviews, _) = build_section(
        DetailSection::Reviews,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    assert!(reviews.is_empty(), "Reviews must be empty when no reviews");

    let (files, _) = build_section(
        DetailSection::Files,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    let text: String =
        files.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
    assert!(text.contains("No files"), "Files placeholder must explain emptiness, got: {text:?}");
}

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

#[test]
fn alt_bg_ranges_alternate_and_stay_within_comments_section() {
    let detail = fixture_pr_detail(0, 0, 0, 3);
    let p = Palette::default();
    let (lines, alt_ranges) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        true,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );

    assert_eq!(
        alt_ranges.len(),
        2,
        "expected 2 alt ranges for 4 items starting off, got {alt_ranges:?}"
    );

    #[allow(clippy::cast_possible_truncation)]
    let total = lines.len() as u16;
    for &(start, end) in &alt_ranges {
        assert!(end <= total, "range {start}..{end} exceeds total lines {total}");
        assert!(start < end, "empty range {start}..{end}");
    }

    let mut sorted = alt_ranges.clone();
    sorted.sort_by_key(|r| r.0);
    for pair in sorted.windows(2) {
        assert!(pair[0].1 <= pair[1].0, "overlapping ranges: {pair:?}");
    }
}

#[test]
fn alt_bg_empty_when_single_comment() {
    let detail = fixture_pr_detail(0, 0, 0, 0);
    let p = Palette::default();
    let (_, alt_ranges) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        true,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    assert!(alt_ranges.is_empty(), "first top-level item should not be tinted, got {alt_ranges:?}");
}

#[test]
fn char_wrap_tint_splits_long_lines_into_full_width_rows() {
    let bg = Color::Rgb(32, 32, 45);
    let original = Line::from(vec![ratatui::text::Span::styled(
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
    let joined: String =
        wrapped.iter().flat_map(|l| l.spans.iter().map(|s| s.content.to_string())).collect();
    assert!(joined.starts_with("abcdefghijklmnopqrstuvwxy"), "content preserved: {joined}");
}

#[test]
fn char_wrap_tint_empty_line_yields_one_padded_row() {
    let bg = Color::Rgb(1, 2, 3);
    let original: Line<'static> = Line::from(vec![]);
    let wrapped = char_wrap_tint(&original, bg, 8);
    assert_eq!(wrapped.len(), 1, "empty line must still produce one tinted row");
    let txt: String = wrapped[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(txt.chars().count(), 8, "row padded to width");
    assert_eq!(wrapped[0].style.bg, Some(bg));
}

#[test]
fn char_wrap_tint_preserves_span_styling_across_split() {
    let bg = Color::Rgb(10, 10, 10);
    let original = Line::from(vec![ratatui::text::Span::styled(
        "red-text-that-spans".to_owned(),
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    )]);
    let wrapped = char_wrap_tint(&original, bg, 8);
    assert_eq!(wrapped.len(), 3, "19 chars at width 8 → 3 rows");
    let mut saw_red_bold = false;
    for line in &wrapped {
        for span in &line.spans {
            if span.content.contains("red")
                || span.content.contains("text")
                || span.content.contains("that")
            {
                assert_eq!(span.style.fg, Some(Color::Red), "fg lost: {span:?}");
                assert!(span.style.add_modifier.contains(Modifier::BOLD), "bold lost: {span:?}");
                saw_red_bold = true;
            }
            assert_eq!(span.style.bg, Some(bg), "bg missing: {span:?}");
        }
    }
    assert!(saw_red_bold, "never saw a styled content span");
}

#[test]
fn tint_line_applies_bg_and_pads_row() {
    let bg = Color::Rgb(32, 32, 45);
    let original = Line::from(vec![
        ratatui::text::Span::styled("hi ", Style::default().fg(Color::Red)),
        ratatui::text::Span::styled("there", Style::default().fg(Color::Blue)),
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

#[test]
fn files_section_renders_cursor_pointed_file_header() {
    let detail = fixture_pr_detail(0, 0, 5, 0);
    let p = Palette::default();

    let (lines, _) = build_section(
        DetailSection::Files,
        &detail,
        0,
        true,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    let text: String =
        lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
    assert!(text.contains("src/file-0.rs"), "files_cursor=0 must show first file path: {text:?}");

    let (lines, _) = build_section(
        DetailSection::Files,
        &detail,
        2,
        true,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    let text: String =
        lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
    assert!(text.contains("src/file-2.rs"), "files_cursor=2 must show third file path: {text:?}");
}

#[test]
fn build_files_overview_produces_one_line_per_file() {
    let p = Palette::default();

    for num_files in [1usize, 3, 7] {
        let detail = fixture_pr_detail(0, 0, num_files, 0);
        let (lines, _) = build_section(
            DetailSection::Files,
            &detail,
            0,
            false,
            false,
            true,
            None,
            &no_expanded(),
            &no_cursor(),
            None,
            0,
            None,
            &p,
            false,
        );

        assert_eq!(
            lines.len(),
            num_files + 1,
            "overview with {num_files} files must produce {num_files}+1 lines, got {}",
            lines.len()
        );

        for (i, line) in lines.iter().take(num_files).enumerate() {
            let text = line_text(line);
            assert!(
                text.contains("src/file-"),
                "overview line {i} must contain file path, got: {text:?}"
            );
        }

        let hint = line_text(&lines[num_files]);
        assert!(
            hint.contains("F open diff"),
            "footer hint must mention 'F open diff', got: {hint:?}"
        );
    }
}

#[test]
fn thread_comment_body_renders_as_markdown() {
    let now = Utc::now();
    let p = Palette::default();
    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
            node_id: "THREAD_node".to_owned(),
            path: "src/lib.rs".to_owned(),
            line: Some(10),
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![ReviewComment {
                node_id: "COMMENT_node".to_owned(),
                author: "bob".to_owned(),
                body_markdown: "# Heading\n\n**bold** text\n\n```rust\nfn f() {}\n```".to_owned(),
                created_at: now,
                diff_hunk: None,
                original_commit_id: None,
            }],
        }],
        issue_comments: vec![],
        commits: vec![],
    };

    let (lines, _) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        true,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );

    let styled_count = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .filter(|s| s.style.add_modifier.contains(Modifier::BOLD) || s.style.bg.is_some())
        .count();

    assert!(styled_count >= 2, "expected >= 2 styled spans (heading + code), got {styled_count}");
}

#[test]
fn files_overview_shows_thread_badge_when_index_reports_threads() {
    // A file with one unresolved, non-outdated thread must show `⚑ 1`
    // somewhere on its overview row. Guards Feature B / 0.1.7.
    use crate::ui::pr_detail::build_thread_index;

    let mut detail = fixture_pr_detail(0, 0, 3, 0);
    // fixture_pr_detail creates files "src/file-0.rs", ..., "src/file-2.rs".
    // Attach one unresolved thread to file-1 and clear the fixture's defaults.
    let now = Utc::now();
    detail.review_threads = vec![ReviewThread {
        node_id: "THREAD_node".to_owned(),
        path: "src/file-1.rs".to_owned(),
        line: Some(3),
        start_line: None,
        is_resolved: false,
        is_outdated: false,
        diff_hunk: None,
        comments: vec![ReviewComment {
            node_id: "COMMENT_node".to_owned(),
            author: "alice".to_owned(),
            body_markdown: "please fix".to_owned(),
            created_at: now,
            diff_hunk: None,
            original_commit_id: None,
        }],
    }];
    let idx = build_thread_index(&detail);
    let p = Palette::default();

    let (lines, _) = super::files::build_files_overview_scoped(&detail, 0, Some(&idx), None, &p);
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect::<Vec<_>>()
        .join("");

    assert!(text.contains("\u{2691} 1"), "overview must show `⚑ 1` for file-1; got: {text}");
}

#[test]
fn files_overview_without_threads_renders_no_badge() {
    // Regression guard: passing `thread_index = None` (the path used when
    // a session-cached PrDetail lacks threads entirely) must not inject
    // spurious glyphs.
    let detail = fixture_pr_detail(0, 0, 2, 0);
    let p = Palette::default();
    let (lines, _) = super::files::build_files_overview_scoped(&detail, 0, None, None, &p);
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect::<Vec<_>>()
        .join("");

    assert!(!text.contains("\u{2691}"), "no threads → no flag glyph; got: {text}");
    assert!(!text.contains("\u{2713}"), "no threads → no check glyph; got: {text}");
}

#[test]
fn outdated_threads_render_in_a_separate_section_with_badge() {
    // A PR with one active + one outdated thread must render both under
    // distinct dividers, and the outdated thread must carry a prominent
    // `[OUTDATED]` badge. Guard for Phase C / 0.1.6.
    let now = Utc::now();
    let p = Palette::default();
    let active = ReviewThread {
        node_id: "THREAD_node".to_owned(),
        path: "src/a.rs".to_owned(),
        line: Some(10),
        start_line: None,
        is_resolved: false,
        is_outdated: false,
        diff_hunk: None,
        comments: vec![ReviewComment {
            node_id: "COMMENT_node".to_owned(),
            author: "alice".to_owned(),
            body_markdown: "still open".to_owned(),
            created_at: now,
            diff_hunk: None,
            original_commit_id: None,
        }],
    };
    let outdated = ReviewThread {
        node_id: "THREAD_node".to_owned(),
        path: "src/b.rs".to_owned(),
        line: Some(5),
        start_line: None,
        is_resolved: true,
        is_outdated: true,
        diff_hunk: None,
        comments: vec![ReviewComment {
            node_id: "COMMENT_node".to_owned(),
            author: "bob".to_owned(),
            body_markdown: "already fixed".to_owned(),
            created_at: now,
            diff_hunk: None,
            original_commit_id: None,
        }],
    };
    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
        review_threads: vec![active, outdated],
        issue_comments: vec![],
        commits: vec![],
    };

    let (lines, _) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect::<Vec<_>>()
        .join("");

    assert!(text.contains("ACTIVE (1)"), "active divider must appear; got: {text}");
    assert!(text.contains("OUTDATED (1)"), "outdated divider must appear");
    assert!(text.contains("[OUTDATED]"), "outdated thread must carry a prominent badge");
    assert!(text.contains("still open"), "active comment body must render");
    assert!(text.contains("already fixed"), "outdated comment body must render when show_outdated");
}

#[test]
fn outdated_threads_hidden_when_show_outdated_false() {
    // When the `z` toggle is off, outdated threads collapse behind a
    // disclosure line that names the count, so the user always knows the
    // threads exist. Silent-drop would be the anti-pattern to avoid.
    let now = Utc::now();
    let p = Palette::default();
    let outdated = ReviewThread {
        node_id: "THREAD_node".to_owned(),
        path: "src/b.rs".to_owned(),
        line: Some(5),
        start_line: None,
        is_resolved: true,
        is_outdated: true,
        diff_hunk: None,
        comments: vec![ReviewComment {
            node_id: "COMMENT_node".to_owned(),
            author: "bob".to_owned(),
            body_markdown: "confidential gossip".to_owned(),
            created_at: now,
            diff_hunk: None,
            original_commit_id: None,
        }],
    };
    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
        review_threads: vec![outdated],
        issue_comments: vec![],
        commits: vec![],
    };

    let (lines, _) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        false,
        false,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect::<Vec<_>>()
        .join("");

    assert!(text.contains("OUTDATED (1)"), "divider stays visible for discoverability");
    assert!(text.contains("[z] show"), "disclosure must include the unhide key: {text}");
    assert!(
        !text.contains("confidential gossip"),
        "hidden outdated body must NOT render; got: {text}"
    );
}

#[test]
fn diff_hunk_excerpt_renders_under_thread_header() {
    // A review thread with a populated `diff_hunk` must emit the parsed
    // excerpt (hunk header + `+`/`-`/context lines) between the thread
    // header and the first comment body. Guards Feature A from 0.1.5.
    let now = Utc::now();
    let p = Palette::default();
    let detail = PrDetail {
            node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
            head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
            node_id: "THREAD_node".to_owned(),
            path: "src/lib.rs".to_owned(),
            line: Some(3),
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: Some(
                "@@ -1,3 +1,4 @@\n fn main() {\n-    let x = 1;\n+    let x = 2;\n+    let y = 3;\n }"
                    .to_owned(),
            ),
            comments: vec![ReviewComment {
                node_id: "COMMENT_node".to_owned(),
                author: "alice".to_owned(),
                body_markdown: "Looks good.".to_owned(),
                created_at: now,
                diff_hunk: None,
            original_commit_id: None,
            }],
        }],
        issue_comments: vec![],
        commits: vec![],
    };

    let (lines, _) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect::<Vec<_>>()
        .join("");

    assert!(text.contains("@@ -1,3 +1,4 @@"), "excerpt must include hunk header; got: {text}");
    assert!(text.contains("let x = 2"), "excerpt must include the added line");
    assert!(text.contains("Looks good."), "the comment body must still render below the excerpt");
}

#[test]
fn thread_without_diff_hunk_renders_cleanly() {
    // A thread with `diff_hunk = None` (old cached payload) must render
    // without a hunk excerpt and without panicking. Fallback path for
    // PRs whose detail was cached before 0.1.5 added the field.
    let now = Utc::now();
    let p = Palette::default();
    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
            node_id: "THREAD_node".to_owned(),
            path: "src/lib.rs".to_owned(),
            line: Some(3),
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![ReviewComment {
                node_id: "COMMENT_node".to_owned(),
                author: "alice".to_owned(),
                body_markdown: "No hunk.".to_owned(),
                created_at: now,
                diff_hunk: None,
                original_commit_id: None,
            }],
        }],
        issue_comments: vec![],
        commits: vec![],
    };

    let (lines, _) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect::<Vec<_>>()
        .join("");

    assert!(!text.contains("@@"), "no hunk must produce no '@@' header; got: {text}");
    assert!(text.contains("No hunk."), "the comment body must still render");
}

#[test]
fn thread_reply_prefix_only_on_non_first_comments() {
    let now = Utc::now();
    let p = Palette::default();
    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
            node_id: "THREAD_node".to_owned(),
            path: "src/lib.rs".to_owned(),
            line: Some(5),
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![
                ReviewComment {
                    node_id: "COMMENT_node".to_owned(),
                    author: "alice".to_owned(),
                    body_markdown: "First comment".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                    original_commit_id: None,
                },
                ReviewComment {
                    node_id: "COMMENT_node".to_owned(),
                    author: "bob".to_owned(),
                    body_markdown: "Second comment".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                    original_commit_id: None,
                },
                ReviewComment {
                    node_id: "COMMENT_node".to_owned(),
                    author: "carol".to_owned(),
                    body_markdown: "Third comment".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                    original_commit_id: None,
                },
            ],
        }],
        issue_comments: vec![],
        commits: vec![],
    };

    let (lines, _) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        true,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );

    let reply_glyph = "\u{21b3} ";
    let has_reply_prefix =
        |line: &Line<'_>| line.spans.iter().any(|s| s.content.contains(reply_glyph));

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

#[test]
fn unresolved_anchor_points_at_thread_header() {
    let now = Utc::now();
    let p = Palette::default();
    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
            node_id: "THREAD_node".to_owned(),
            path: "src/lib.rs".to_owned(),
            line: Some(42),
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![ReviewComment {
                node_id: "COMMENT_node".to_owned(),
                author: "bob".to_owned(),
                body_markdown: "Needs refactor.".to_owned(),
                created_at: now,
                diff_hunk: None,
                original_commit_id: None,
            }],
        }],
        issue_comments: vec![],
        commits: vec![],
    };

    let (lines, unresolved, _) = comments_lines(&detail, true, true, None, &p, false);

    assert_eq!(unresolved.len(), 1, "expected exactly 1 unresolved anchor");
    let anchor = unresolved[0] as usize;
    assert!(anchor < lines.len(), "anchor out of bounds");

    let header_text = line_text(&lines[anchor]);
    assert!(
        header_text.contains("src/lib.rs"),
        "anchor line should contain file path, got: {header_text:?}"
    );
    assert!(
        header_text.contains('\u{2691}'), // ⚑
        "anchor line should contain ⚑ glyph, got: {header_text:?}"
    );
}

#[test]
fn replies_render_in_accent_alt() {
    let now = Utc::now();
    let p = Palette::default();
    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
            node_id: "THREAD_node".to_owned(),
            path: "src/lib.rs".to_owned(),
            line: Some(1),
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![
                ReviewComment {
                    node_id: "COMMENT_node".to_owned(),
                    author: "opener".to_owned(),
                    body_markdown: "Opening thought.".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                    original_commit_id: None,
                },
                ReviewComment {
                    node_id: "COMMENT_node".to_owned(),
                    author: "replier".to_owned(),
                    body_markdown: "Counter-point.".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                    original_commit_id: None,
                },
            ],
        }],
        issue_comments: vec![],
        commits: vec![],
    };

    let (lines, _) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        true,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );

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

    let reply_gutter_count = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .filter(|s| s.content.as_ref().contains('\u{2502}') && s.style.fg == Some(p.accent_alt))
        .count();
    assert!(reply_gutter_count > 0, "expected at least one accent_alt gutter rail for the reply");
}

#[test]
fn collapsed_long_comment_shows_expand_hint() {
    let now = Utc::now();
    let p = Palette::default();
    let long_body = (0..10).map(|i| format!("Paragraph {i}.")).collect::<Vec<_>>().join("\n\n");

    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
            node_id: "THREAD_node".to_owned(),
            path: "src/lib.rs".to_owned(),
            line: Some(1),
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![ReviewComment {
                node_id: "COMMENT_node".to_owned(),
                author: "alice".to_owned(),
                body_markdown: long_body,
                created_at: now,
                diff_hunk: None,
                original_commit_id: None,
            }],
        }],
        issue_comments: vec![],
        commits: vec![],
    };

    let (lines, _) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );

    let has_expand_hint = lines.iter().any(|l| line_text(l).contains("[m] expand"));
    assert!(has_expand_hint, "collapsed long comment must show [m] expand hint");
}

#[test]
fn issue_comments_render_markdown_styles() {
    let now = Utc::now();
    let p = Palette::default();
    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
        issue_comments: vec![IssueComment {
            node_id: "COMMENT_node".to_owned(),
            author: "dave".to_owned(),
            body_markdown: "**important** and `code_snippet`".to_owned(),
            created_at: now,
        }],
        commits: vec![],
    };

    let (lines, _) = build_section(
        DetailSection::Comments,
        &detail,
        0,
        false,
        true,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );

    let has_bold = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .any(|s| s.content.contains("important") && s.style.add_modifier.contains(Modifier::BOLD));

    let has_code = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .any(|s| s.content.contains("code_snippet") && s.style.bg == Some(p.code_bg));

    assert!(has_bold, "issue comment body must render **bold** with BOLD modifier");
    assert!(has_code, "issue comment body must render `code_snippet` with code_bg");
}

#[test]
fn has_content_reflects_fixture_content() {
    let empty = fixture_pr_detail(0, 0, 0, 0);
    assert!(DetailSection::Description.has_content(&empty), "Description always has content");
    assert!(!DetailSection::Checks.has_content(&empty));
    assert!(!DetailSection::Reviews.has_content(&empty));
    assert!(!DetailSection::Files.has_content(&empty));
    assert!(DetailSection::Comments.has_content(&empty));
}

#[test]
fn section_labels_are_correct() {
    assert_eq!(DetailSection::Description.label(), "Description");
    assert_eq!(DetailSection::Checks.label(), "Checks");
    assert_eq!(DetailSection::Reviews.label(), "Reviews");
    assert_eq!(DetailSection::Files.label(), "Files");
    assert_eq!(DetailSection::Comments.label(), "Comments");
}

// ── 0.1.8: inline thread-card tests ──────────────────────────────────────────

/// Helper: build a minimal `PrDetail` with a patch so `build_files_diff` has
/// a diff to walk. The patch has one context line at new-file line 10 and one
/// added line at new-file line 11, so threads anchored to line 10 can be
/// tested inline.
fn fixture_diff_detail_with_thread(line: Option<u32>, outdated: bool) -> PrDetail {
    let now = Utc::now();
    PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "owner/repo".to_owned(),
        number: 42,
        title: "Diff thread test".to_owned(),
        url: "https://github.com/owner/repo/pull/42".to_owned(),
        author: "alice".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat/inline".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        is_draft: false,
        additions: 1,
        deletions: 0,
        changed_files_count: 1,
        updated_at: now,
        created_at: now,
        merged: false,
        files: vec![crate::github::detail::FileChange {
            path: "src/lib.rs".to_owned(),
            additions: 1,
            deletions: 0,
            change_kind: FileChangeKind::Modified,
            // One hunk: context line at new-lineno 10, then an added line.
            patch: Some(
                "@@ -9,2 +9,3 @@\n fn existing() {}\n+fn new_fn() {}\n fn after() {}\n".to_owned(),
            ),
        }],
        check_runs: vec![],
        reviews: vec![],
        review_threads: vec![ReviewThread {
            node_id: "THREAD_node".to_owned(),
            path: "src/lib.rs".to_owned(),
            line,
            start_line: None,
            is_resolved: false,
            is_outdated: outdated,
            diff_hunk: None,
            comments: vec![ReviewComment {
                node_id: "COMMENT_node".to_owned(),
                author: "bob".to_owned(),
                body_markdown: "thread body text".to_owned(),
                created_at: now,
                diff_hunk: None,
                original_commit_id: None,
            }],
        }],
        issue_comments: vec![],
        commits: vec![],
    }
}

#[test]
fn inline_thread_card_collapsed_emits_single_summary_row() {
    // A thread at line 10 of a diff → exactly one card line (not more) when collapsed.
    // The patch puts a context line at new-lineno 9 and an added line at 10, 11.
    // We need to find line 10 in the parsed diff to anchor the thread.
    use super::files::build_files_diff;
    use super::thread_index::ThreadIndex;

    let detail = fixture_diff_detail_with_thread(Some(10), false);
    let index = ThreadIndex::build(&detail.review_threads);
    let expanded: HashSet<(String, u32)> = HashSet::new();
    let cursor: RefCell<Option<(String, u32)>> = RefCell::new(None);
    let p = Palette::default();

    let (lines, _) =
        build_files_diff(&detail, 0, Some(&index), &expanded, &cursor, None, &p, false);

    // Collect all text to check for the collapsed card marker.
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect::<Vec<_>>()
        .join("");

    assert!(text.contains("[t] expand"), "collapsed card must show '[t] expand' hint; got: {text}");

    // Count cards by their unique leading 13-space pad (`CARD_PAD`). Filtering
    // on `"[t] expand"` alone collides with the file-level `[t] expand at
    // cursor` hint line introduced in 0.1.11 and would count both.
    let card_lines: Vec<_> = lines
        .iter()
        .filter(|l| {
            let t = l.spans.iter().map(|s| s.content.as_ref()).collect::<String>();
            t.starts_with("             ") && t.contains("[t] expand")
        })
        .collect();
    assert_eq!(card_lines.len(), 1, "exactly one collapsed card line; got {}", card_lines.len());
}

#[test]
fn inline_thread_card_expanded_emits_body_rows() {
    // Same thread but with the anchor in `expanded` — output must have at
    // least anchor-line + header-line + body-line = 3 rows from the thread card.
    use super::files::build_files_diff;
    use super::thread_index::ThreadIndex;

    let detail = fixture_diff_detail_with_thread(Some(10), false);
    let index = ThreadIndex::build(&detail.review_threads);
    let mut expanded: HashSet<(String, u32)> = HashSet::new();
    expanded.insert(("src/lib.rs".to_owned(), 10));
    let cursor: RefCell<Option<(String, u32)>> = RefCell::new(None);
    let p = Palette::default();

    let (lines, _) =
        build_files_diff(&detail, 0, Some(&index), &expanded, &cursor, None, &p, false);

    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect::<Vec<_>>()
        .join("");

    // Expanded header row must carry the collapse hint.
    assert!(
        text.contains("[t] collapse"),
        "expanded card must show '[t] collapse' hint; got: {text}"
    );
    // The comment body should appear.
    assert!(text.contains("thread body text"), "expanded card must show comment body; got: {text}");

    // At least 2 card rows: the expanded header + at least one body line.
    let card_rows: Vec<_> = lines
        .iter()
        .filter(|l| {
            let t: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            t.contains("[t] collapse") || t.contains("thread body text")
        })
        .collect();
    assert!(
        card_rows.len() >= 2,
        "expanded card must have >= 2 content rows (header + body), got {}",
        card_rows.len()
    );
}

#[test]
fn overflow_block_renders_outdated_and_file_level() {
    // A thread with `line=None` (file-level) and one outdated thread must both
    // appear after the last hunk in the overflow block, NOT inserted mid-diff.
    use super::files::build_files_diff;
    use super::thread_index::ThreadIndex;
    use chrono::Utc;

    let now = Utc::now();
    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "owner/repo".to_owned(),
        number: 99,
        title: "Overflow test".to_owned(),
        url: "https://github.com/owner/repo/pull/99".to_owned(),
        author: "alice".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        is_draft: false,
        additions: 1,
        deletions: 0,
        changed_files_count: 1,
        updated_at: now,
        created_at: now,
        merged: false,
        files: vec![crate::github::detail::FileChange {
            path: "src/lib.rs".to_owned(),
            additions: 1,
            deletions: 0,
            change_kind: FileChangeKind::Modified,
            patch: Some("@@ -1,1 +1,2 @@\n context\n+added\n".to_owned()),
        }],
        check_runs: vec![],
        reviews: vec![],
        review_threads: vec![
            // File-level thread: line == None
            ReviewThread {
                node_id: "THREAD_node".to_owned(),
                path: "src/lib.rs".to_owned(),
                line: None,
                start_line: None,
                is_resolved: false,
                is_outdated: false,
                diff_hunk: None,
                comments: vec![ReviewComment {
                    node_id: "COMMENT_node".to_owned(),
                    author: "bob".to_owned(),
                    body_markdown: "file-level comment".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                    original_commit_id: None,
                }],
            },
            // Outdated thread: line == Some(5) but is_outdated == true
            ReviewThread {
                node_id: "THREAD_node".to_owned(),
                path: "src/lib.rs".to_owned(),
                line: Some(5),
                start_line: None,
                is_resolved: false,
                is_outdated: true,
                diff_hunk: None,
                comments: vec![ReviewComment {
                    node_id: "COMMENT_node".to_owned(),
                    author: "carol".to_owned(),
                    body_markdown: "outdated comment".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                    original_commit_id: None,
                }],
            },
        ],
        issue_comments: vec![],
        commits: vec![],
    };

    let index = ThreadIndex::build(&detail.review_threads);
    let expanded: HashSet<(String, u32)> = HashSet::new();
    let cursor: RefCell<Option<(String, u32)>> = RefCell::new(None);
    let p = Palette::default();

    let (lines, _) =
        build_files_diff(&detail, 0, Some(&index), &expanded, &cursor, None, &p, false);

    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect::<Vec<_>>()
        .join("");

    // The overflow divider must appear.
    assert!(
        text.contains("File-level & outdated threads"),
        "overflow divider must appear; got: {text}"
    );

    // Neither thread must appear mid-diff (before the hunk lines): verify
    // that the diff hunk header appears before the overflow divider.
    let hunk_pos = text.find("@@").expect("hunk header must be present");
    let overflow_pos = text.find("File-level & outdated threads").expect("overflow block");
    assert!(
        hunk_pos < overflow_pos,
        "overflow block must come after the diff hunk; hunk_pos={hunk_pos}, overflow_pos={overflow_pos}"
    );

    // Both threads produce collapsed cards in the overflow block. The
    // file-level `[t] expand at cursor` hint introduced in 0.1.11 also
    // contains `[t] expand`, so scope the count to text AFTER the overflow
    // divider — cards only appear in that region.
    let expand_hints = text[overflow_pos..].matches("[t] expand").count();
    assert_eq!(expand_hints, 2, "overflow block must render 2 collapsed cards; got {expand_hints}");
}

#[test]
fn toggle_keybind_round_trip() {
    // Tests the `t` key toggle logic by verifying the `pr_detail_expanded_threads`
    // set state before and after a simulated cursor presence. Since `handle_key`
    // is `pub(super)` and not reachable from this module, we exercise the same
    // business logic by calling `build_files_diff` twice — once collapsed (empty
    // expanded set), once with the anchor pre-inserted — and checking that the
    // cursor `RefCell` records the anchor that the `t` handler would use.
    use super::files::build_files_diff;
    use super::thread_index::ThreadIndex;

    let detail = fixture_diff_detail_with_thread(Some(10), false);
    let index = ThreadIndex::build(&detail.review_threads);
    let p = Palette::default();

    // ── Collapsed state ───────────────────────────────────────────────────────
    let expanded_empty: HashSet<(String, u32)> = HashSet::new();
    let cursor_a: RefCell<Option<(String, u32)>> = RefCell::new(None);
    build_files_diff(&detail, 0, Some(&index), &expanded_empty, &cursor_a, None, &p, false);

    // The renderer must have written the anchor to the cursor cell.
    let anchor = cursor_a.borrow().clone();
    assert_eq!(
        anchor,
        Some(("src/lib.rs".to_owned(), 10)),
        "renderer must record the thread-anchor line in diff_cursor; got {anchor:?}"
    );

    // ── Simulate toggle-on: insert the anchor into expanded ───────────────────
    let mut expanded_one = HashSet::new();
    expanded_one.insert(("src/lib.rs".to_owned(), 10));
    let cursor_b: RefCell<Option<(String, u32)>> = RefCell::new(None);
    let (lines_expanded, _) =
        build_files_diff(&detail, 0, Some(&index), &expanded_one, &cursor_b, None, &p, false);

    let expanded_text: String =
        lines_expanded.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
    assert!(
        expanded_text.contains("[t] collapse"),
        "after toggle-on the card must show '[t] collapse'; got: {expanded_text}"
    );

    // ── Simulate toggle-off: remove the anchor ────────────────────────────────
    let cursor_c: RefCell<Option<(String, u32)>> = RefCell::new(None);
    let (lines_collapsed, _) =
        build_files_diff(&detail, 0, Some(&index), &expanded_empty, &cursor_c, None, &p, false);

    let collapsed_text: String =
        lines_collapsed.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
    assert!(
        collapsed_text.contains("[t] expand"),
        "after toggle-off the card must show '[t] expand'; got: {collapsed_text}"
    );
}

// ── 0.2.0: Commits section tests ──────────────────────────────────────────────

/// Build a `PrDetail` populated with `n` commits having explicit dates so the
/// sort behaviour can be verified without relying on wall-clock `Utc::now()`.
pub fn fixture_pr_detail_with_commits(n: usize) -> PrDetail {
    use chrono::TimeZone;

    let now = Utc::now();
    let commits = (0..n)
        .map(|i| {
            // Space commits 1 hour apart, oldest first in construction order.
            // After `raw_pr_to_detail`-style sort (newest-first) index 0 is the
            // commit with the largest timestamp, i.e. `i == n-1` construction order.
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            let committed_at =
                Utc.timestamp_opt(1_700_000_000 + i as i64 * 3600, 0).single().unwrap_or(now);
            let sha = format!("{i:040x}");
            let short_sha: String = sha.chars().take(7).collect();
            #[allow(clippy::cast_possible_truncation)]
            let additions = (i as u32) * 10;
            #[allow(clippy::cast_possible_truncation)]
            let deletions = i as u32;
            PrCommit {
                sha,
                short_sha,
                headline: format!("commit message {i}"),
                author: format!("author-{i}"),
                committed_at,
                additions,
                deletions,
                changed_files: 1,
                check_state: Some(crate::github::types::CheckState::Success),
            }
        })
        .collect::<Vec<_>>();

    let mut detail = fixture_pr_detail(0, 0, 0, 0);
    detail.commits = commits;
    detail
}

#[test]
fn commits_section_renders_one_line_per_commit() {
    // Header + blank spacer + 3 commit rows = at least 4 lines.
    let detail = fixture_pr_detail_with_commits(3);
    let p = Palette::default();

    let (lines, alt_ranges) = build_section(
        DetailSection::Commits,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );

    assert!(
        lines.len() >= 4,
        "expected >= 4 lines (header + spacer + 3 rows), got {}",
        lines.len()
    );
    assert!(alt_ranges.is_empty(), "Commits section must return no alt-bg ranges");

    // Each commit row must contain its short SHA (first 7 chars of 40-char SHA).
    for commit in &detail.commits {
        let sha_found = lines.iter().any(|l| {
            let t: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            t.contains(&commit.short_sha)
        });
        assert!(sha_found, "commit {} short SHA must appear in output", commit.short_sha);
    }
}

#[test]
fn commits_sorted_newest_first() {
    // Construct 3 commits with explicit dates; newest has the highest timestamp.
    use chrono::TimeZone;

    let old_at = Utc.timestamp_opt(1_000_000, 0).unwrap();
    let mid_at = Utc.timestamp_opt(2_000_000, 0).unwrap();
    let new_at = Utc.timestamp_opt(3_000_000, 0).unwrap();

    let commits_unsorted = vec![
        PrCommit {
            sha: "a".repeat(40),
            short_sha: "aaaaaaa".to_owned(),
            headline: "oldest".to_owned(),
            author: "dev".to_owned(),
            committed_at: old_at,
            additions: 1,
            deletions: 0,
            changed_files: 1,
            check_state: None,
        },
        PrCommit {
            sha: "b".repeat(40),
            short_sha: "bbbbbbb".to_owned(),
            headline: "newest".to_owned(),
            author: "dev".to_owned(),
            committed_at: new_at,
            additions: 2,
            deletions: 0,
            changed_files: 1,
            check_state: None,
        },
        PrCommit {
            sha: "c".repeat(40),
            short_sha: "ccccccc".to_owned(),
            headline: "middle".to_owned(),
            author: "dev".to_owned(),
            committed_at: mid_at,
            additions: 3,
            deletions: 0,
            changed_files: 1,
            check_state: None,
        },
    ];

    let mut detail = fixture_pr_detail(0, 0, 0, 0);
    detail.commits = commits_unsorted;

    // Apply the same sort that `raw_pr_to_detail` applies.
    detail.commits.sort_unstable_by(|a, b| b.committed_at.cmp(&a.committed_at));

    assert_eq!(
        detail.commits[0].sha,
        "b".repeat(40),
        "index 0 must be the newest commit; got sha {}",
        detail.commits[0].sha
    );
    assert_eq!(
        detail.commits[2].sha,
        "a".repeat(40),
        "index 2 must be the oldest commit; got sha {}",
        detail.commits[2].sha
    );
}

#[test]
fn commits_section_hidden_when_empty() {
    let detail = fixture_pr_detail(0, 0, 0, 0); // commits: vec![]
    assert!(detail.commits.is_empty(), "fixture must have no commits for this test");
    assert!(
        !DetailSection::Commits.has_content(&detail),
        "has_content must return false when commits is empty"
    );
}

#[test]
fn commits_section_key_is_sixth() {
    assert_eq!(DetailSection::ALL.len(), 6, "ALL must have exactly 6 sections");
    assert_eq!(DetailSection::ALL[5], DetailSection::Commits, "index 5 (6th) must be Commits");
    assert_eq!(DetailSection::Commits.label(), "Commits");
}

// ── 0.2.2: Scoped Comments + per-commit CI ───────────────────────────────────

/// Helper: build a `ReviewComment` with an explicit `original_commit_id`.
fn make_review_comment(author: &str, body: &str, commit_oid: Option<&str>) -> ReviewComment {
    ReviewComment {
        node_id: "COMMENT_node".to_owned(),
        author: author.to_owned(),
        body_markdown: body.to_owned(),
        created_at: Utc::now(),
        diff_hunk: None,
        original_commit_id: commit_oid.map(str::to_owned),
    }
}

/// Helper: build a minimal `ReviewThread` with one comment originating on
/// the given SHA (or `None` for old cached payloads).
fn make_thread(path: &str, author: &str, body: &str, commit_oid: Option<&str>) -> ReviewThread {
    ReviewThread {
        node_id: "THREAD_node".to_owned(),
        path: path.to_owned(),
        line: Some(1),
        start_line: None,
        is_resolved: false,
        is_outdated: false,
        diff_hunk: None,
        comments: vec![make_review_comment(author, body, commit_oid)],
    }
}

#[test]
fn scoped_comments_filter_by_origin_commit() {
    // Two threads on different SHAs; scope to sha_a — only alice's thread
    // should render, not bob's.  Issue comments (carol) always appear.
    let now = Utc::now();
    let p = Palette::default();

    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
        review_threads: vec![
            make_thread("src/a.rs", "alice", "alice thread body", Some("aaaaaaa_sha")),
            make_thread("src/b.rs", "bob", "bob thread body", Some("bbbbbbb_sha")),
        ],
        issue_comments: vec![IssueComment {
            node_id: "COMMENT_node".to_owned(),
            author: "carol".to_owned(),
            body_markdown: "carol issue comment".to_owned(),
            created_at: now,
        }],
        commits: vec![],
    };

    let (lines, _, _) = comments_lines(&detail, true, true, Some("aaaaaaa_sha"), &p, false);
    let text: String =
        lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();

    assert!(
        text.contains("alice thread body"),
        "alice's thread must appear when scoped to her SHA"
    );
    assert!(!text.contains("bob thread body"), "bob's thread must NOT appear in alice's scope");
    assert!(
        text.contains("carol issue comment"),
        "issue comments always appear regardless of scope"
    );
}

#[test]
fn scoped_comments_show_scope_hint() {
    // When a scope SHA is set, the ◈ hint row must appear in the output.
    let now = Utc::now();
    let p = Palette::default();

    let sha = "a3f7b2caabbcc";
    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
        review_threads: vec![make_thread("src/a.rs", "alice", "thread body", Some(sha))],
        issue_comments: vec![],
        commits: vec![],
    };

    let (lines, _, _) = comments_lines(&detail, true, true, Some(sha), &p, false);
    let text: String =
        lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();

    // The hint must contain "Scoped to" and the first 7 chars of the SHA.
    assert!(text.contains("Scoped to"), "scope hint must contain 'Scoped to'; got: {text}");
    assert!(text.contains(&sha[..7]), "scope hint must contain the 7-char short SHA; got: {text}");
    assert!(text.contains("H returns to HEAD"), "scope hint must mention H key; got: {text}");
}

#[test]
fn scoped_comments_empty_scope_shows_notice() {
    // When no threads originate on the scoped SHA, a muted notice must appear.
    let now = Utc::now();
    let p = Palette::default();

    let detail = PrDetail {
        node_id: "PR_node".to_owned(),
        repo: "r".to_owned(),
        number: 1,
        title: "T".to_owned(),
        url: "u".to_owned(),
        author: "a".to_owned(),
        body_markdown: String::new(),
        base_ref: "main".to_owned(),
        head_ref: "feat".to_owned(),
        head_oid: "0123456789abcdef0123456789abcdef01234567".to_owned(),
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
        // Thread on a different SHA — the scope will miss.
        review_threads: vec![make_thread("src/a.rs", "alice", "thread body", Some("other_sha"))],
        issue_comments: vec![IssueComment {
            node_id: "COMMENT_node".to_owned(),
            author: "dave".to_owned(),
            body_markdown: "some issue comment".to_owned(),
            created_at: now,
        }],
        commits: vec![],
    };

    let (lines, _, _) = comments_lines(&detail, true, true, Some("nonexistent_sha"), &p, false);
    let text: String =
        lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();

    assert!(
        text.contains("No review threads originated on this commit"),
        "empty-scope notice must appear; got: {text}"
    );
    // Issue comments still render below the notice.
    assert!(text.contains("some issue comment"), "issue comments must still appear; got: {text}");
}

#[test]
fn per_commit_ci_glyph_rendered_in_list() {
    // A commit with `check_state: Some(CheckState::Failure)` must produce a
    // rendered row containing the failure CI glyph character.
    use crate::github::types::CheckState;
    use crate::ui::glyphs;
    use chrono::TimeZone;

    let now = Utc.timestamp_opt(1_700_000_000, 0).single().unwrap_or_else(Utc::now);
    let failure_commit = PrCommit {
        sha: "f".repeat(40),
        short_sha: "fffffff".to_owned(),
        headline: "failing commit".to_owned(),
        author: "dev".to_owned(),
        committed_at: now,
        additions: 5,
        deletions: 2,
        changed_files: 1,
        check_state: Some(CheckState::Failure),
    };

    let mut detail = fixture_pr_detail(0, 0, 0, 0);
    detail.commits = vec![failure_commit];

    let p = Palette::default();
    let (lines, _) = build_section(
        DetailSection::Commits,
        &detail,
        0,
        false,
        false,
        true,
        None,
        &no_expanded(),
        &no_cursor(),
        None,
        0,
        None,
        &p,
        false,
    );

    // The failure glyph (non-ASCII mode).
    let (expected_glyph, _) = glyphs::ci_glyph(Some(CheckState::Failure), false);
    let glyph_str = expected_glyph.to_string();

    let rendered_text: String =
        lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();

    assert!(
        rendered_text.contains(&glyph_str),
        "failure CI glyph '{expected_glyph}' must appear in commit list row; got: {rendered_text}"
    );
}
