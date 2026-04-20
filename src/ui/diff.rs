//! Unified-diff parser and renderer for the PR detail right pane.
//!
//! The two public entry points are:
//! - [`parse_unified_diff`] — converts a raw patch string into a [`DiffFile`]
//! - [`render_diff`] — converts a [`DiffFile`] into `Vec<Line<'static>>` using
//!   the active [`Palette`]
//!
//! The module is intentionally free of any app state; the only inputs are a
//! patch string and a palette reference.
#![allow(dead_code)]

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::theme::Palette;

// ── Public types ──────────────────────────────────────────────────────────────

/// A parsed unified-diff file: an ordered sequence of change hunks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffFile {
    pub hunks: Vec<DiffHunk>,
}

/// One `@@ … @@` section with its diff lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// First line number in the old (left-hand) file covered by this hunk.
    pub old_start: u32,
    /// Number of lines from the old file covered by this hunk.
    pub old_count: u32,
    /// First line number in the new (right-hand) file covered by this hunk.
    pub new_start: u32,
    /// Number of lines from the new file covered by this hunk.
    pub new_count: u32,
    /// Optional context text that follows the trailing `@@` (e.g. a function
    /// signature). Empty when the generator omitted it.
    pub section: String,
    /// The diff lines belonging to this hunk, in order.
    pub lines: Vec<DiffLine>,
}

/// Classification of a single diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    /// Unchanged line present in both old and new files (`' '` prefix).
    Context,
    /// Line present only in the new file (`'+'` prefix).
    Added,
    /// Line present only in the old file (`'-'` prefix).
    Removed,
    /// Pseudo-line indicating that the preceding line has no trailing newline
    /// (`'\\'` prefix in the patch, e.g. `\ No newline at end of file`).
    NoNewline,
}

/// A single line within a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// Line content with the leading `+`/`-`/` ` prefix stripped.
    /// For [`DiffLineKind::NoNewline`] lines the leading `\` is also stripped.
    pub content: String,
    /// Line number in the old file, or `None` for [`DiffLineKind::Added`] lines.
    pub old_lineno: Option<u32>,
    /// Line number in the new file, or `None` for [`DiffLineKind::Removed`] lines.
    pub new_lineno: Option<u32>,
}

// ── Public helpers ────────────────────────────────────────────────────────────

/// Parse a `@@ -old_start[,old_count] +new_start[,new_count] @@ [section]`
/// hunk header line.
///
/// Returns `(old_start, old_count, new_start, new_count, section)` or `None`
/// if the line does not match the expected pattern.  When the count is absent
/// (e.g. `@@ -1 +1 @@`) it defaults to `1`.
///
/// # Examples
///
/// ```
/// # use gh_tui::ui::diff::parse_hunk_header;
/// let result = parse_hunk_header("@@ -1,5 +1,7 @@");
/// assert_eq!(result, Some((1, 5, 1, 7, String::new())));
/// ```
pub fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32, String)> {
    // Expected shape: "@@ -A[,B] +C[,D] @@ optional section text"
    // We locate the two `@@` delimiters and parse what is between them.
    let line = line.trim();
    if !line.starts_with("@@") {
        return None;
    }
    // Find the closing `@@` — it must appear after the opening one.
    // Use offset 2 so we skip the leading `@@` itself.
    let closing = line[2..].find("@@")?;
    // `closing` is relative to `line[2..]`, so add 2 to get the absolute index.
    let closing_abs = closing + 2;
    let inner = line[2..closing_abs].trim(); // e.g. "-1,5 +1,7"
    let section = line[closing_abs + 2..].trim().to_owned(); // text after the closing @@

    // Split inner into the two range specs.
    let mut parts = inner.split_whitespace();
    let old_spec = parts.next()?; // "-A[,B]"
    let new_spec = parts.next()?; // "+C[,D]"

    let (old_start, old_count) = parse_range_spec(old_spec, '-')?;
    let (new_start, new_count) = parse_range_spec(new_spec, '+')?;

    Some((old_start, old_count, new_start, new_count, section))
}

// ── Public parser ─────────────────────────────────────────────────────────────

/// Parse a complete unified-diff patch string into a [`DiffFile`].
///
/// Lines before the first hunk header (`@@ … @@`) are silently skipped; this
/// handles the `diff --git …`, `index …`, `--- a/…`, `+++ b/…` preamble that
/// Git emits.  Malformed input never panics — worst case it produces an empty
/// or partial result.
///
/// # Examples
///
/// ```
/// # use gh_tui::ui::diff::parse_unified_diff;
/// let patch = "@@ -1,2 +1,3 @@\n context\n+added\n-removed\n";
/// let file = parse_unified_diff(patch);
/// assert_eq!(file.hunks.len(), 1);
/// assert_eq!(file.hunks[0].lines.len(), 3);
/// ```
pub fn parse_unified_diff(patch: &str) -> DiffFile {
    if patch.is_empty() {
        return DiffFile::default();
    }

    let mut hunks: Vec<DiffHunk> = Vec::new();
    // Cursors into the old and new line number sequences, updated as we walk
    // through each line in the current hunk.
    let mut old_cursor: u32 = 0;
    let mut new_cursor: u32 = 0;

    for raw_line in patch.lines() {
        if let Some((old_start, old_count, new_start, new_count, section)) =
            parse_hunk_header(raw_line)
        {
            // Start a new hunk and reset the line-number cursors.
            old_cursor = old_start;
            new_cursor = new_start;
            hunks.push(DiffHunk {
                old_start,
                old_count,
                new_start,
                new_count,
                section,
                lines: Vec::new(),
            });
            continue;
        }

        // Lines before any hunk header are silently skipped (preamble).
        let Some(hunk) = hunks.last_mut() else {
            continue;
        };

        // Classify the line by its first character.
        let mut chars = raw_line.chars();
        let prefix = chars.next();
        // Content is everything after the first character.
        let content = chars.as_str().to_owned();

        let diff_line = match prefix {
            Some(' ') => {
                // Context line: present in both old and new.
                let line = DiffLine {
                    kind: DiffLineKind::Context,
                    content,
                    old_lineno: Some(old_cursor),
                    new_lineno: Some(new_cursor),
                };
                old_cursor += 1;
                new_cursor += 1;
                line
            }
            Some('+') => {
                // Added line: only in the new file.
                let line = DiffLine {
                    kind: DiffLineKind::Added,
                    content,
                    old_lineno: None,
                    new_lineno: Some(new_cursor),
                };
                new_cursor += 1;
                line
            }
            Some('-') => {
                // Removed line: only in the old file.
                let line = DiffLine {
                    kind: DiffLineKind::Removed,
                    content,
                    old_lineno: Some(old_cursor),
                    new_lineno: None,
                };
                old_cursor += 1;
                line
            }
            Some('\\') => {
                // "\ No newline at end of file" — the leading backslash is the
                // prefix; the rest is the content.  Neither line counter advances.
                DiffLine {
                    kind: DiffLineKind::NoNewline,
                    content,
                    old_lineno: None,
                    new_lineno: None,
                }
            }
            _ => {
                // Defensive fallback: unrecognised line inside a hunk is treated
                // as context without advancing line numbers.
                DiffLine {
                    kind: DiffLineKind::Context,
                    content: raw_line.to_owned(),
                    old_lineno: Some(old_cursor),
                    new_lineno: Some(new_cursor),
                }
            }
        };

        hunk.lines.push(diff_line);
    }

    DiffFile { hunks }
}

// ── Public renderer ───────────────────────────────────────────────────────────

/// Render a [`DiffFile`] into a sequence of styled [`Line`]s.
///
/// Visual layout (per diff line):
///
/// ```text
///   <old_lineno:5>  <new_lineno:5>  <kind_prefix:1>  <content>
/// ```
///
/// - Line numbers are right-justified in 5-wide columns, in `palette.muted`.
///   Empty slots (e.g. old lineno for an Added line) render as five spaces.
/// - Kind prefix: `' '` (context), `'+'` (added), `'-'` (removed).
/// - Hunk headers are rendered before the hunk's lines in `palette.accent` bold
///   with the section text in `palette.dim`.
/// - A blank separator line is inserted between consecutive hunks.
/// - An empty [`DiffFile`] produces a single placeholder line.
#[allow(clippy::too_many_lines)]
pub fn render_diff(file: &DiffFile, palette: &Palette) -> Vec<Line<'static>> {
    if file.hunks.is_empty() {
        return vec![Line::from(Span::styled(
            "(no changes to show)",
            Style::default().fg(palette.dim),
        ))];
    }

    let mut output: Vec<Line<'static>> = Vec::new();

    for (hunk_idx, hunk) in file.hunks.iter().enumerate() {
        // Blank separator between consecutive hunks for readability.
        if hunk_idx > 0 {
            output.push(Line::default());
        }

        // ── Hunk header ───────────────────────────────────────────────────────
        // Format: `@@ -old_start,old_count +new_start,new_count @@`
        let header_coords = format!(
            "@@ -{},{} +{},{} @@",
            hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
        );
        let mut header_spans = vec![Span::styled(
            header_coords,
            Style::default().fg(palette.accent).add_modifier(Modifier::BOLD),
        )];
        if !hunk.section.is_empty() {
            header_spans.push(Span::raw(" "));
            header_spans.push(Span::styled(hunk.section.clone(), Style::default().fg(palette.dim)));
        }
        output.push(Line::from(header_spans));

        // ── Diff lines ────────────────────────────────────────────────────────
        for diff_line in &hunk.lines {
            let line = render_diff_line(diff_line, palette);
            output.push(line);
        }
    }

    output
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Parse a unified-diff range specifier like `-1,5` or `+3` into
/// `(start, count)`.  The `expected_prefix` is `'-'` for old-file specs and
/// `'+'` for new-file specs.  Missing count defaults to `1`.
fn parse_range_spec(spec: &str, expected_prefix: char) -> Option<(u32, u32)> {
    let spec = spec.strip_prefix(expected_prefix)?;
    if let Some((start_str, count_str)) = spec.split_once(',') {
        let start = start_str.parse().ok()?;
        let count = count_str.parse().ok()?;
        Some((start, count))
    } else {
        let start = spec.parse().ok()?;
        // When the count is absent the hunk contains exactly 1 line.
        Some((start, 1))
    }
}

/// Format a line-number slot as a right-justified 5-wide string, or five
/// spaces when no line number is present.
fn format_lineno(lineno: Option<u32>) -> String {
    match lineno {
        Some(n) => format!("{n:>5}"),
        None => "     ".to_owned(),
    }
}

/// Render a single [`DiffLine`] as a styled ratatui [`Line`].
///
/// Promoted to `pub(crate)` so `pr_detail::files::render_diff_with_threads`
/// can call it directly per line without going through the full `render_diff`
/// path. This keeps `diff.rs` free of thread concerns (single responsibility).
pub(crate) fn render_diff_line(diff_line: &DiffLine, palette: &Palette) -> Line<'static> {
    // The `NoNewline` pseudo-line gets a distinct minimal layout — no line
    // numbers, no prefix column — just the message in a muted style.
    if diff_line.kind == DiffLineKind::NoNewline {
        // The content stored is everything after the leading `\`, so we
        // reconstruct the conventional display form.
        let text = format!("\\ {}", diff_line.content);
        return Line::from(Span::styled(text, Style::default().fg(palette.dim)));
    }

    let old_str = format_lineno(diff_line.old_lineno);
    let new_str = format_lineno(diff_line.new_lineno);

    // One trailing space after each lineno column for visual separation.
    let lineno_style = Style::default().fg(palette.muted);

    let (prefix_char, content_style) = match diff_line.kind {
        DiffLineKind::Added => ('+', Style::default().fg(palette.git_new)),
        DiffLineKind::Removed => ('-', Style::default().fg(palette.danger)),
        DiffLineKind::Context => (' ', Style::default().fg(palette.foreground)),
        // NoNewline is handled above and never reaches this point.
        DiffLineKind::NoNewline => unreachable!("NoNewline handled above"),
    };

    Line::from(vec![
        Span::styled(old_str, lineno_style),
        Span::styled(" ", lineno_style),
        Span::styled(new_str, lineno_style),
        Span::styled(" ", lineno_style),
        Span::styled(prefix_char.to_string(), content_style),
        Span::raw(" "),
        Span::styled(diff_line.content.clone(), content_style),
    ])
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::theme::{Palette, Theme};

    fn default_palette() -> Palette {
        Palette::from_theme(Theme::Default)
    }

    // ── parse_hunk_header ─────────────────────────────────────────────────────

    #[test]
    fn parse_hunk_header_basic() {
        let result = parse_hunk_header("@@ -1,5 +1,7 @@");
        assert_eq!(result, Some((1, 5, 1, 7, String::new())));
    }

    #[test]
    fn parse_hunk_header_with_section() {
        let result = parse_hunk_header("@@ -10,3 +12,3 @@ fn main()");
        assert_eq!(result, Some((10, 3, 12, 3, "fn main()".to_owned())));
    }

    #[test]
    fn parse_hunk_header_single_line_counts_default_to_one() {
        let result = parse_hunk_header("@@ -1 +1 @@");
        assert_eq!(result, Some((1, 1, 1, 1, String::new())));
    }

    #[test]
    fn parse_hunk_header_non_hunk_line_returns_none() {
        assert_eq!(parse_hunk_header("--- a/src/lib.rs"), None);
        assert_eq!(parse_hunk_header("+++ b/src/lib.rs"), None);
        assert_eq!(parse_hunk_header("diff --git a/x b/x"), None);
    }

    // ── parse_unified_diff ────────────────────────────────────────────────────

    #[test]
    fn parse_empty_patch_returns_empty_diffile() {
        let file = parse_unified_diff("");
        assert_eq!(file, DiffFile::default());
        assert!(file.hunks.is_empty());
    }

    #[test]
    fn parse_malformed_lines_before_hunk_are_skipped() {
        let patch = "\
diff --git a/x b/x
index abc123..def456 100644
--- a/x
+++ b/x
@@ -1,2 +1,3 @@
 context
+added
";
        let file = parse_unified_diff(patch);
        assert_eq!(file.hunks.len(), 1, "preamble lines must not create a hunk");
        assert_eq!(file.hunks[0].lines.len(), 2);
    }

    #[test]
    fn parse_single_hunk_context_add_remove() {
        // A hunk with one context, one added, and one removed line.
        // Verify that line numbers advance correctly:
        //   old advances on context + removed
        //   new advances on context + added
        let patch = "\
@@ -5,3 +5,3 @@
 context line
+added line
-removed line
";
        let file = parse_unified_diff(patch);
        assert_eq!(file.hunks.len(), 1);
        let lines = &file.hunks[0].lines;
        assert_eq!(lines.len(), 3);

        // Context: both counters advance from 5
        assert_eq!(lines[0].kind, DiffLineKind::Context);
        assert_eq!(lines[0].old_lineno, Some(5));
        assert_eq!(lines[0].new_lineno, Some(5));

        // Added: old counter unchanged (was advanced by context to 6),
        // new counter was also advanced by context to 6 and then bumped to 7.
        assert_eq!(lines[1].kind, DiffLineKind::Added);
        assert_eq!(lines[1].old_lineno, None);
        assert_eq!(lines[1].new_lineno, Some(6));

        // Removed: old counter is at 6 (advanced by context), new is None.
        assert_eq!(lines[2].kind, DiffLineKind::Removed);
        assert_eq!(lines[2].old_lineno, Some(6));
        assert_eq!(lines[2].new_lineno, None);
    }

    #[test]
    fn parse_multi_hunk() {
        let patch = "\
@@ -1,2 +1,3 @@
 context
+added
@@ -10,2 +11,2 @@
-removed
 context
";
        let file = parse_unified_diff(patch);
        assert_eq!(file.hunks.len(), 2, "two hunk headers should produce two hunks");
        assert_eq!(file.hunks[0].lines.len(), 2);
        assert_eq!(file.hunks[1].lines.len(), 2);
        // Second hunk starts at the right offset.
        assert_eq!(file.hunks[1].old_start, 10);
        assert_eq!(file.hunks[1].new_start, 11);
    }

    #[test]
    fn parse_no_newline_at_eof_marker() {
        let patch = "\
@@ -1 +1 @@
-old content
\\ No newline at end of file
+new content
\\ No newline at end of file
";
        let file = parse_unified_diff(patch);
        assert_eq!(file.hunks.len(), 1);
        let lines = &file.hunks[0].lines;

        // Expect: Removed, NoNewline, Added, NoNewline
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].kind, DiffLineKind::Removed);

        let no_nl = &lines[1];
        assert_eq!(no_nl.kind, DiffLineKind::NoNewline);
        // Line numbers must not advance for NoNewline pseudo-lines.
        assert_eq!(no_nl.old_lineno, None);
        assert_eq!(no_nl.new_lineno, None);

        assert_eq!(lines[2].kind, DiffLineKind::Added);
        assert_eq!(lines[3].kind, DiffLineKind::NoNewline);
    }

    // ── render_diff ───────────────────────────────────────────────────────────

    #[test]
    fn render_diff_empty_returns_placeholder() {
        let palette = default_palette();
        let file = DiffFile::default();
        let lines = render_diff(&file, &palette);
        assert_eq!(lines.len(), 1);
        let first_span = lines[0].spans.first().expect("placeholder line must have a span");
        assert!(
            first_span.content.contains("no changes to show"),
            "placeholder text mismatch: {:?}",
            first_span.content
        );
    }

    #[test]
    fn render_diff_colours_additions_and_deletions() {
        let palette = default_palette();
        let patch = "\
@@ -1,2 +1,2 @@
 context line
+added line
-removed line
";
        let file = parse_unified_diff(patch);
        let lines = render_diff(&file, &palette);

        // Collect all fg colors from every span in every rendered line.
        let all_fgs: Vec<ratatui::style::Color> =
            lines.iter().flat_map(|l| l.spans.iter()).filter_map(|s| s.style.fg).collect();

        assert!(
            all_fgs.contains(&palette.git_new),
            "no span with git_new fg found; fgs = {all_fgs:?}"
        );
        assert!(
            all_fgs.contains(&palette.danger),
            "no span with danger fg found; fgs = {all_fgs:?}"
        );
        assert!(
            all_fgs.contains(&palette.foreground),
            "no span with foreground fg found; fgs = {all_fgs:?}"
        );
    }

    #[test]
    fn render_diff_hunk_header_styled_in_accent_bold() {
        let palette = default_palette();
        let patch = "@@ -1,1 +1,1 @@\n context\n";
        let file = parse_unified_diff(patch);
        let lines = render_diff(&file, &palette);

        // The very first line is the hunk header.
        let header_line = &lines[0];
        let first_span = header_line.spans.first().expect("header line must have spans");

        assert_eq!(
            first_span.style.fg,
            Some(palette.accent),
            "hunk header first span fg must be accent"
        );
        assert!(
            first_span.style.add_modifier.contains(Modifier::BOLD),
            "hunk header first span must be bold"
        );
    }

    #[test]
    fn render_diff_section_context_appears_in_dim() {
        let palette = default_palette();
        let patch = "@@ -1,1 +1,1 @@ fn example()\n context\n";
        let file = parse_unified_diff(patch);
        let lines = render_diff(&file, &palette);

        let header_line = &lines[0];
        // Last span is the section text.
        let section_span = header_line.spans.last().expect("header must have section span");
        assert_eq!(
            section_span.style.fg,
            Some(palette.dim),
            "section context span must use palette.dim"
        );
        assert!(
            section_span.content.contains("fn example()"),
            "section text not found in header spans"
        );
    }

    #[test]
    fn render_diff_blank_line_between_hunks() {
        let palette = default_palette();
        let patch = "\
@@ -1,1 +1,1 @@
 first
@@ -10,1 +10,1 @@
 second
";
        let file = parse_unified_diff(patch);
        assert_eq!(file.hunks.len(), 2);
        let lines = render_diff(&file, &palette);

        // Layout: header1, diff_line1, blank, header2, diff_line2
        // Find the blank separator — it must exist somewhere between the two
        // hunk headers.
        let blank_count = lines.iter().filter(|l| l.spans.is_empty()).count();
        assert_eq!(blank_count, 1, "expected exactly one blank separator between hunks");
    }
}
