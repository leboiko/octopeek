//! Unit tests for the `pr_detail` module.
//!
//! Moved wholesale from the bottom of the original monolithic `pr_detail.rs`.

// See the same note in `src/app/tests.rs`: tests lean on `.unwrap()` /
// `.expect()` for assertion-site failures; production code keeps the
// lints set in Cargo.toml.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

use crate::github::detail::{
    DetailedCheck, DetailedReview, FileChange, FileChangeKind, IssueComment, PrDetail,
    ReviewComment, ReviewThread,
};
use crate::github::types::ReviewState;
use crate::theme::Palette;
use chrono::Utc;

use super::DetailSection;
use super::comments::comments_lines;
use super::header::{build_header, char_wrap_tint, tint_line};
use super::sections::build_section;

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
            path: format!("src/file-{i}.rs"),
            #[allow(clippy::cast_possible_truncation)]
            line: Some((i as u32 + 1) * 5),
            start_line: None,
            is_resolved: i % 3 == 0,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![ReviewComment {
                author: format!("user-{i}"),
                body_markdown: format!("Comment {i}"),
                created_at: now,
                diff_hunk: None,
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

#[test]
fn build_section_non_empty_sections_have_lines() {
    let detail = fixture_pr_detail(2, 1, 3, 1);
    let p = Palette::default();

    let (desc, _) =
        build_section(DetailSection::Description, &detail, 0, false, false, true, &p, false);
    assert!(!desc.is_empty(), "Description must produce lines when body is non-empty");

    let (checks, _) =
        build_section(DetailSection::Checks, &detail, 0, false, false, true, &p, false);
    assert!(!checks.is_empty(), "Checks must produce lines when check_runs is non-empty");

    let (reviews, _) =
        build_section(DetailSection::Reviews, &detail, 0, false, false, true, &p, false);
    assert!(!reviews.is_empty(), "Reviews must produce lines when reviews is non-empty");

    let (files, _) = build_section(DetailSection::Files, &detail, 0, false, false, true, &p, false);
    assert!(!files.is_empty(), "Files must produce lines when files is non-empty");

    let (comments, _) =
        build_section(DetailSection::Comments, &detail, 0, false, false, true, &p, false);
    assert!(!comments.is_empty(), "Comments must produce lines when threads are present");
}

#[test]
fn build_section_empty_sections_have_no_lines() {
    let detail = fixture_pr_detail(0, 0, 0, 0); // only issue comment, no threads
    let p = Palette::default();

    let (checks, _) =
        build_section(DetailSection::Checks, &detail, 0, false, false, true, &p, false);
    assert!(checks.is_empty(), "Checks must be empty when no check_runs");

    let (reviews, _) =
        build_section(DetailSection::Reviews, &detail, 0, false, false, true, &p, false);
    assert!(reviews.is_empty(), "Reviews must be empty when no reviews");

    let (files, _) = build_section(DetailSection::Files, &detail, 0, false, false, true, &p, false);
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
    let (lines, alt_ranges) =
        build_section(DetailSection::Comments, &detail, 0, false, true, true, &p, false);

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
    let (_, alt_ranges) =
        build_section(DetailSection::Comments, &detail, 0, false, true, true, &p, false);
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

    let (lines, _) = build_section(DetailSection::Files, &detail, 0, true, false, true, &p, false);
    let text: String =
        lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
    assert!(text.contains("src/file-0.rs"), "files_cursor=0 must show first file path: {text:?}");

    let (lines, _) = build_section(DetailSection::Files, &detail, 2, true, false, true, &p, false);
    let text: String =
        lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
    assert!(text.contains("src/file-2.rs"), "files_cursor=2 must show third file path: {text:?}");
}

#[test]
fn build_files_overview_produces_one_line_per_file() {
    let p = Palette::default();

    for num_files in [1usize, 3, 7] {
        let detail = fixture_pr_detail(0, 0, num_files, 0);
        let (lines, _) =
            build_section(DetailSection::Files, &detail, 0, false, false, true, &p, false);

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
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![ReviewComment {
                author: "bob".to_owned(),
                body_markdown: "# Heading\n\n**bold** text\n\n```rust\nfn f() {}\n```".to_owned(),
                created_at: now,
                diff_hunk: None,
            }],
        }],
        issue_comments: vec![],
    };

    let (lines, _) =
        build_section(DetailSection::Comments, &detail, 0, false, true, true, &p, false);

    let styled_count = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .filter(|s| s.style.add_modifier.contains(Modifier::BOLD) || s.style.bg.is_some())
        .count();

    assert!(styled_count >= 2, "expected >= 2 styled spans (heading + code), got {styled_count}");
}

#[test]
fn outdated_threads_render_in_a_separate_section_with_badge() {
    // A PR with one active + one outdated thread must render both under
    // distinct dividers, and the outdated thread must carry a prominent
    // `[OUTDATED]` badge. Guard for Phase C / 0.1.6.
    let now = Utc::now();
    let p = Palette::default();
    let active = ReviewThread {
        path: "src/a.rs".to_owned(),
        line: Some(10),
        start_line: None,
        is_resolved: false,
        is_outdated: false,
        diff_hunk: None,
        comments: vec![ReviewComment {
            author: "alice".to_owned(),
            body_markdown: "still open".to_owned(),
            created_at: now,
            diff_hunk: None,
        }],
    };
    let outdated = ReviewThread {
        path: "src/b.rs".to_owned(),
        line: Some(5),
        start_line: None,
        is_resolved: true,
        is_outdated: true,
        diff_hunk: None,
        comments: vec![ReviewComment {
            author: "bob".to_owned(),
            body_markdown: "already fixed".to_owned(),
            created_at: now,
            diff_hunk: None,
        }],
    };
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
        review_threads: vec![active, outdated],
        issue_comments: vec![],
    };

    let (lines, _) =
        build_section(DetailSection::Comments, &detail, 0, false, false, true, &p, false);
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
        path: "src/b.rs".to_owned(),
        line: Some(5),
        start_line: None,
        is_resolved: true,
        is_outdated: true,
        diff_hunk: None,
        comments: vec![ReviewComment {
            author: "bob".to_owned(),
            body_markdown: "confidential gossip".to_owned(),
            created_at: now,
            diff_hunk: None,
        }],
    };
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
        review_threads: vec![outdated],
        issue_comments: vec![],
    };

    let (lines, _) =
        build_section(DetailSection::Comments, &detail, 0, false, false, false, &p, false);
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
            line: Some(3),
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: Some(
                "@@ -1,3 +1,4 @@\n fn main() {\n-    let x = 1;\n+    let x = 2;\n+    let y = 3;\n }"
                    .to_owned(),
            ),
            comments: vec![ReviewComment {
                author: "alice".to_owned(),
                body_markdown: "Looks good.".to_owned(),
                created_at: now,
                diff_hunk: None,
            }],
        }],
        issue_comments: vec![],
    };

    let (lines, _) =
        build_section(DetailSection::Comments, &detail, 0, false, false, true, &p, false);
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
            line: Some(3),
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![ReviewComment {
                author: "alice".to_owned(),
                body_markdown: "No hunk.".to_owned(),
                created_at: now,
                diff_hunk: None,
            }],
        }],
        issue_comments: vec![],
    };

    let (lines, _) =
        build_section(DetailSection::Comments, &detail, 0, false, false, true, &p, false);
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
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![
                ReviewComment {
                    author: "alice".to_owned(),
                    body_markdown: "First comment".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                },
                ReviewComment {
                    author: "bob".to_owned(),
                    body_markdown: "Second comment".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                },
                ReviewComment {
                    author: "carol".to_owned(),
                    body_markdown: "Third comment".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                },
            ],
        }],
        issue_comments: vec![],
    };

    let (lines, _) =
        build_section(DetailSection::Comments, &detail, 0, false, true, true, &p, false);

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
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![ReviewComment {
                author: "bob".to_owned(),
                body_markdown: "Needs refactor.".to_owned(),
                created_at: now,
                diff_hunk: None,
            }],
        }],
        issue_comments: vec![],
    };

    let (lines, unresolved, _) = comments_lines(&detail, true, true, &p, false);

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
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![
                ReviewComment {
                    author: "opener".to_owned(),
                    body_markdown: "Opening thought.".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                },
                ReviewComment {
                    author: "replier".to_owned(),
                    body_markdown: "Counter-point.".to_owned(),
                    created_at: now,
                    diff_hunk: None,
                },
            ],
        }],
        issue_comments: vec![],
    };

    let (lines, _) =
        build_section(DetailSection::Comments, &detail, 0, false, true, true, &p, false);

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
            start_line: None,
            is_resolved: false,
            is_outdated: false,
            diff_hunk: None,
            comments: vec![ReviewComment {
                author: "alice".to_owned(),
                body_markdown: long_body,
                created_at: now,
                diff_hunk: None,
            }],
        }],
        issue_comments: vec![],
    };

    let (lines, _) =
        build_section(DetailSection::Comments, &detail, 0, false, false, true, &p, false);

    let has_expand_hint = lines.iter().any(|l| line_text(l).contains("[m] expand"));
    assert!(has_expand_hint, "collapsed long comment must show [m] expand hint");
}

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
        issue_comments: vec![IssueComment {
            author: "dave".to_owned(),
            body_markdown: "**important** and `code_snippet`".to_owned(),
            created_at: now,
        }],
    };

    let (lines, _) =
        build_section(DetailSection::Comments, &detail, 0, false, true, true, &p, false);

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
