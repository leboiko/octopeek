//! GitHub-flavored Markdown renderer for the TUI.
//!
//! [`render_markdown`] converts a Markdown string into a `Vec<Line<'static>>`
//! ready to be placed in a [`ratatui::widgets::Paragraph`].  The renderer
//! handles the subset of GFM that matters for PR/issue bodies:
//! headings, paragraphs, inline styles, code (inline + fenced), links,
//! lists, blockquotes, tables, and line breaks.
//!
//! Syntect grammar/theme sets are loaded once via [`std::sync::LazyLock`];
//! subsequent calls reuse the cached data.
//!
//! These items are intentionally `#[allow(dead_code)]` — the public API is
//! wired by the Phase 4 detail-UI agent which lives in a parallel branch.
#![allow(dead_code)]

use std::sync::LazyLock;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet, util::LinesWithEndings,
};

use crate::theme::Palette;

// ── Lazily-loaded syntect globals ─────────────────────────────────────────────

/// Syntect syntax definitions loaded once at first use.
///
/// `load_defaults_nonewlines()` omits trailing newlines from each line, which
/// maps cleanly onto ratatui `Span` slices.
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_nonewlines);

/// Syntect built-in theme set loaded once at first use.
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Above this size, fenced code blocks render with plain code styling instead
/// of syntect highlighting. Large pasted logs and generated code snippets are
/// common in review comments; highlighting them synchronously can stall the UI.
const MAX_SYNTAX_HIGHLIGHT_BYTES: usize = 24 * 1024;

/// Collapsed comment bodies render from this bounded source preview.
const COMMENT_PREVIEW_SOURCE_LINES: usize = 40;
const COMMENT_PREVIEW_SOURCE_CHARS: usize = 4_000;
const COMMENT_PREVIEW_RENDERED_LINES: usize = 6;

// ── Internal builder types ────────────────────────────────────────────────────

/// Determines where an `Event::Text` payload should be routed.
///
/// Computed once per `Event::Text` via [`Builder::text_sink`] and matched
/// exhaustively, so adding a fourth context forces a compile error that
/// surfaces both `text_sink()` and the `Event::Text` arm simultaneously.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextSink {
    /// Ordinary paragraph / heading / blockquote / list-item inline text.
    Inline,
    /// Text inside a fenced (or indented) code block — buffered for syntect.
    CodeBlock,
    /// Text inside a GFM table cell — collected into `table_cell_spans`.
    TableCell,
}

/// Accumulated text and style for a single inline run within a line.
struct InlineSpan {
    text: String,
    style: Style,
}

impl InlineSpan {
    fn new(text: impl Into<String>, style: Style) -> Self {
        Self { text: text.into(), style }
    }
}

/// Mutable state threaded through the event loop.
struct Builder<'p> {
    palette: &'p Palette,
    /// Completed lines ready to emit.
    lines: Vec<Line<'static>>,
    /// Inline spans for the line currently being assembled.
    current_spans: Vec<InlineSpan>,
    /// Stacked inline style modifiers (bold, italic, strikethrough).
    /// Each push/pop corresponds to an opening/closing inline tag.
    style_stack: Vec<Style>,
    /// Nesting depth for blockquotes.
    bq_depth: usize,
    /// `Some(lang)` while inside a fenced code block.
    code_block_lang: Option<String>,
    /// Accumulated raw text for the current fenced code block.
    code_block_buf: String,
    /// Nesting depth for ordered/unordered lists (0 = top-level).
    list_depth: usize,
    /// Current ordered-list item counter (`None` when in an unordered list).
    list_counter: Vec<Option<u64>>,
    /// `true` while rendering inside a table block.
    in_table: bool,
    /// Column alignments for the current table, from the markdown header line.
    table_alignments: Vec<Alignment>,
    /// `true` between `Start(TableHead)` and `End(TableHead)`.
    table_in_header: bool,
    /// Spans accumulated for the current cell (one `Span` per text/style run).
    table_cell_spans: Vec<Span<'static>>,
    /// Cells accumulated for the row currently being built.
    table_current_row: Vec<Vec<Span<'static>>>,
    /// Completed header row (from `End(TableHead)`).
    table_header_row: Option<Vec<Vec<Span<'static>>>>,
    /// Completed body rows.
    table_body_rows: Vec<Vec<Vec<Span<'static>>>>,
}

impl<'p> Builder<'p> {
    fn new(palette: &'p Palette) -> Self {
        Self {
            palette,
            lines: Vec::new(),
            current_spans: Vec::new(),
            // Base style: the palette's default foreground.
            style_stack: vec![Style::default().fg(palette.foreground)],
            bq_depth: 0,
            code_block_lang: None,
            code_block_buf: String::new(),
            list_depth: 0,
            list_counter: Vec::new(),
            in_table: false,
            table_alignments: Vec::new(),
            table_in_header: false,
            table_cell_spans: Vec::new(),
            table_current_row: Vec::new(),
            table_header_row: None,
            table_body_rows: Vec::new(),
        }
    }

    // ── Text routing ─────────────────────────────────────────────────────────

    /// Compute where the current `Event::Text` payload should be routed.
    ///
    /// Priority mirrors the original if-chain in `handle_event`:
    /// a fenced-code-block context takes precedence over a table context
    /// (though pulldown-cmark never emits `Text` inside both simultaneously).
    fn text_sink(&self) -> TextSink {
        if self.code_block_lang.is_some() {
            TextSink::CodeBlock
        } else if self.in_table {
            TextSink::TableCell
        } else {
            TextSink::Inline
        }
    }

    // ── Style helpers ─────────────────────────────────────────────────────────

    /// The style currently at the top of the stack.
    fn current_style(&self) -> Style {
        // SAFETY: `style_stack` is initialized with one element and we never
        // pop the last item; `last()` returning `None` is unreachable.
        self.style_stack.last().copied().unwrap_or_default()
    }

    /// Push a new style derived from the current top, applying `patch`.
    fn push_style(&mut self, patch: impl Fn(Style) -> Style) {
        let top = self.current_style();
        self.style_stack.push(patch(top));
    }

    /// Pop the most recently pushed style.
    fn pop_style(&mut self) {
        // Always keep at least the base style on the stack.
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    // ── Line assembly ─────────────────────────────────────────────────────────

    /// Append `text` as a span with the current style.
    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let style = self.current_style();
        self.current_spans.push(InlineSpan::new(text, style));
    }

    /// Commit `current_spans` as a finished [`Line`] and reset.
    ///
    /// If inside a blockquote, prepend the border glyph(s).
    fn flush_line(&mut self) {
        let mut spans: Vec<Span<'static>> = Vec::new();

        // Blockquote border prefix: one `▌` per nesting level.
        for _ in 0..self.bq_depth {
            spans.push(Span::styled("▌ ", Style::default().fg(self.palette.block_quote_border)));
        }

        for InlineSpan { text, style } in self.current_spans.drain(..) {
            spans.push(Span::styled(text, style));
        }
        self.lines.push(Line::from(spans));
    }

    // ── Block-level renderers ─────────────────────────────────────────────────

    /// Emit a heading line with appropriate color and modifiers.
    ///
    /// We intentionally do NOT re-prefix the text with `#`/`##`/`...` — the
    /// hashes are markdown syntax, not content, and TUI users expect the
    /// renderer to hide them the way a browser does. For visual weight we
    /// rely on bold + colour, and add a rule line under H1/H2 so they stand
    /// out as section breaks even when flanked by long paragraphs.
    fn emit_heading(&mut self, level: HeadingLevel) {
        let p = self.palette;
        let color = heading_color(level, p);
        let mods = match level {
            HeadingLevel::H1 | HeadingLevel::H2 | HeadingLevel::H3 => Modifier::BOLD,
            _ => Modifier::empty(),
        };

        let heading_style = Style::default().fg(color).add_modifier(mods);

        // Measure the heading's display width so the rule underline matches.
        let display_width: usize = self
            .current_spans
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.text.as_str()))
            .sum();

        // Re-colour every accumulated span with the heading style.
        let spans: Vec<Span<'static>> =
            self.current_spans.drain(..).map(|s| Span::styled(s.text, heading_style)).collect();
        self.lines.push(Line::from(spans));

        // Rule underline for H1/H2 only — deeper levels stay compact.
        let rule_char = match level {
            HeadingLevel::H1 => Some('\u{2501}'), // ━ heavy
            HeadingLevel::H2 => Some('\u{2500}'), // ─ light
            _ => None,
        };
        if let Some(ch) = rule_char {
            // Clamp rule width so very long headings don't produce absurdly
            // long lines; 48 is enough to visually anchor most titles.
            let len = display_width.clamp(3, 48);
            self.lines.push(Line::from(Span::styled(
                ch.to_string().repeat(len),
                Style::default().fg(color),
            )));
        }
        self.lines.push(Line::from(vec![]));
    }

    /// Emit a fenced code block using syntect for syntax highlighting.
    ///
    /// Falls back to plain `palette.code_fg` when the language is unknown or
    /// when syntect fails to highlight.
    fn emit_code_block(&mut self) {
        let lang = self.code_block_lang.take().unwrap_or_default();
        let source = std::mem::take(&mut self.code_block_buf);
        let p = self.palette;
        let theme_name = p.syntax_theme_name_from_code_bg();

        let highlighted = try_highlight_code(&source, &lang, theme_name, &SYNTAX_SET, &THEME_SET);

        match highlighted {
            Some(lines) => self.lines.extend(lines),
            None => {
                // Fallback: plain colour, no syntax highlighting.
                for raw in source.lines() {
                    self.lines.push(Line::from(vec![Span::styled(
                        raw.to_owned(),
                        Style::default().fg(p.code_fg).bg(p.code_bg),
                    )]));
                }
            }
        }

        // Blank line after the block.
        self.lines.push(Line::from(vec![]));
    }

    /// Emit the accumulated table as bordered lines using box-drawing chars.
    ///
    /// Ported from the sibling `markdown-reader` project's `layout_table`
    /// (`fair_share_widths` + `border_line` + `span_cell_line`), simplified
    /// to work without a known viewport width: we target `TABLE_TARGET_TOTAL`
    /// columns, wide enough for most PR/issue tables and degrades cleanly
    /// (via ratatui's Paragraph wrapping) on narrower terminals.
    ///
    /// Preserves each cell's inline styling (bold / emphasis / inline code)
    /// so tables containing richly-styled cells look like they do elsewhere
    /// in the rendered markdown.
    fn emit_table(&mut self) {
        // Total render width we target when laying out the table. This sits
        // at module scope to keep the constant visible if a future caller
        // wants to thread an actual viewport width through.
        const TABLE_TARGET_TOTAL: usize = 100;
        let header = self.table_header_row.take().unwrap_or_default();
        let rows = std::mem::take(&mut self.table_body_rows);
        let alignments = std::mem::take(&mut self.table_alignments);

        let num_cols = header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
        if num_cols == 0 {
            return;
        }

        // Natural width per column: the max display-width of any cell.
        let mut natural_widths: Vec<usize> = vec![0; num_cols];
        let measure = |cell: &[Span<'static>]| -> usize {
            cell.iter().map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref())).sum()
        };
        for (i, cell) in header.iter().enumerate().take(num_cols) {
            natural_widths[i] = natural_widths[i].max(measure(cell));
        }
        for row in &rows {
            for (i, cell) in row.iter().enumerate().take(num_cols) {
                natural_widths[i] = natural_widths[i].max(measure(cell));
            }
        }

        // Target content width — total layout is content + 2 padding per col
        // + (num_cols + 1) vertical border chars.
        let target = TABLE_TARGET_TOTAL
            .saturating_sub(num_cols + 1) // borders
            .saturating_sub(2 * num_cols); // padding
        let col_widths = fair_share_widths(&natural_widths, num_cols, target);

        let p = self.palette;
        let border_style = Style::default().fg(p.table_border);
        let header_style = Style::default().fg(p.table_header).add_modifier(Modifier::BOLD);
        let cell_style = Style::default().fg(p.foreground);

        // Top border.
        self.lines.push(border_line(
            '\u{250C}',
            '\u{2500}',
            '\u{252C}',
            '\u{2510}',
            &col_widths,
            border_style,
        ));
        // Header row — falls back to an empty cell when a table has no
        // header (rare but possible with some renderers).
        self.lines.push(span_cell_line(
            &header,
            &col_widths,
            &alignments,
            border_style,
            header_style,
            num_cols,
            p,
        ));
        // Header/body separator.
        self.lines.push(border_line(
            '\u{251C}',
            '\u{2500}',
            '\u{253C}',
            '\u{2524}',
            &col_widths,
            border_style,
        ));
        // Body rows.
        for row in &rows {
            self.lines.push(span_cell_line(
                row,
                &col_widths,
                &alignments,
                border_style,
                cell_style,
                num_cols,
                p,
            ));
        }
        // Bottom border.
        self.lines.push(border_line(
            '\u{2514}',
            '\u{2500}',
            '\u{2534}',
            '\u{2518}',
            &col_widths,
            border_style,
        ));
    }
}

// ── Syntect theme name heuristic ──────────────────────────────────────────────

impl Palette {
    /// Derive a syntect theme name from the palette's `code_bg` color.
    ///
    /// Uses luminance of `code_bg`: bright backgrounds (light themes) map to
    /// `InspiredGitHub`; dark backgrounds map to `base16-ocean.dark`.
    /// This avoids threading the `Theme` enum through `Palette` callers.
    pub(crate) fn syntax_theme_name_from_code_bg(&self) -> &'static str {
        match self.code_bg {
            Color::Rgb(r, g, b) => {
                // ITU-R BT.709 luminance formula.
                let lum = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
                if lum > 180.0 { "InspiredGitHub" } else { "base16-ocean.dark" }
            }
            _ => "base16-ocean.dark",
        }
    }
}

// ── Table layout helpers (ported from markdown-reader) ───────────────────────

/// Compute column widths using a proportional fair-share algorithm.
///
/// If all naturals fit within `target`, returns natural widths (clamped to
/// at least 1). Otherwise every column gets a minimum of `min(6, natural)`,
/// and remaining space is distributed proportionally to each column's
/// excess over its minimum.
fn fair_share_widths(natural_widths: &[usize], num_cols: usize, target: usize) -> Vec<usize> {
    let naturals: Vec<usize> =
        (0..num_cols).map(|i| natural_widths.get(i).copied().unwrap_or(1).max(1)).collect();

    let total_natural: usize = naturals.iter().sum();
    if total_natural <= target {
        return naturals;
    }

    let mins: Vec<usize> = naturals.iter().map(|&n| n.clamp(1, 6)).collect();
    let total_min: usize = mins.iter().sum();

    if total_min >= target {
        let per_col = (target / num_cols).max(1);
        return mins.iter().map(|&m| m.min(per_col).max(1)).collect();
    }

    let remaining = target - total_min;
    let total_excess: usize = naturals.iter().zip(&mins).map(|(&n, &m)| n.saturating_sub(m)).sum();

    let mut widths = mins.clone();
    for (i, (&natural, &min)) in naturals.iter().zip(&mins).enumerate() {
        let excess = natural.saturating_sub(min);
        // `checked_div` returns `None` when `total_excess == 0`, i.e. every
        // column already sits at its natural width. In that case no extra
        // space is distributed — same behaviour as the previous
        // `if total_excess > 0` guard.
        let extra = (excess * remaining).checked_div(total_excess).unwrap_or(0);
        widths[i] = (min + extra).min(natural);
    }
    widths
}

/// Render a horizontal border (top `┌─┬─┐`, separator `├─┼─┤`, or bottom
/// `└─┴─┘`). Each column's span is `width + 2` chars to account for the
/// single-space padding on both sides of cell content.
fn border_line(
    left: char,
    fill: char,
    mid: char,
    right: char,
    col_widths: &[usize],
    style: Style,
) -> Line<'static> {
    let mut s = String::with_capacity(col_widths.iter().sum::<usize>() + col_widths.len() * 4);
    s.push(left);
    for (i, &w) in col_widths.iter().enumerate() {
        for _ in 0..(w + 2) {
            s.push(fill);
        }
        if i + 1 < col_widths.len() {
            s.push(mid);
        }
    }
    s.push(right);
    Line::from(Span::styled(s, style))
}

/// Render one table row (header or body), preserving each cell's inline
/// styling. `cell_fill_style` applies only to padding; original span styles
/// are retained on the content spans themselves.
#[allow(clippy::too_many_arguments)]
fn span_cell_line(
    cells: &[Vec<Span<'static>>],
    col_widths: &[usize],
    alignments: &[Alignment],
    border_style: Style,
    cell_fill_style: Style,
    num_cols: usize,
    palette: &Palette,
) -> Line<'static> {
    let empty: Vec<Span<'static>> = Vec::new();
    let mut out: Vec<Span<'static>> = Vec::with_capacity(num_cols * 4 + 1);
    out.push(Span::styled("\u{2502}".to_owned(), border_style)); // │

    for (i, &w) in col_widths.iter().enumerate().take(num_cols) {
        let cell = cells.get(i).unwrap_or(&empty);
        let cell_w: usize =
            cell.iter().map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref())).sum();
        let alignment = alignments.get(i).copied().unwrap_or(Alignment::None);

        // Left space padding.
        out.push(Span::styled(" ".to_owned(), cell_fill_style));

        if cell_w <= w {
            let padding = w - cell_w;
            let (left_pad, right_pad) = match alignment {
                Alignment::Right => (padding, 0),
                Alignment::Center => (padding / 2, padding - padding / 2),
                Alignment::Left | Alignment::None => (0, padding),
            };
            if left_pad > 0 {
                out.push(Span::styled(" ".repeat(left_pad), cell_fill_style));
            }
            out.extend(cell.iter().cloned());
            if right_pad > 0 {
                out.push(Span::styled(" ".repeat(right_pad), cell_fill_style));
            }
        } else {
            out.extend(truncate_spans(cell, w, palette));
        }

        // Right space padding + column border.
        out.push(Span::styled(" \u{2502}".to_owned(), border_style)); // ` │`
    }

    Line::from(out)
}

/// Truncate `spans` to fit in `max_width` display columns, appending an
/// ellipsis (`…`) in the palette's dim style when truncation occurs.
fn truncate_spans(
    spans: &[Span<'static>],
    max_width: usize,
    palette: &Palette,
) -> Vec<Span<'static>> {
    if max_width == 0 {
        return Vec::new();
    }
    let budget = max_width.saturating_sub(1); // reserve one cell for the ellipsis
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for span in spans {
        let w = unicode_width::UnicodeWidthStr::width(span.content.as_ref());
        if used + w <= budget {
            out.push(span.clone());
            used += w;
            continue;
        }
        // Partial: cut this span at the last char boundary that fits.
        let remaining = budget.saturating_sub(used);
        let mut acc = String::new();
        let mut acc_w = 0usize;
        for ch in span.content.chars() {
            let cw = unicode_width::UnicodeWidthStr::width(ch.to_string().as_str());
            if acc_w + cw > remaining {
                break;
            }
            acc.push(ch);
            acc_w += cw;
        }
        if !acc.is_empty() {
            out.push(Span::styled(acc, span.style));
        }
        break;
    }
    out.push(Span::styled(
        "\u{2026}".to_owned(), // …
        Style::default().fg(palette.dim),
    ));
    out
}

/// Pick the palette colour for a heading level.
///
/// Extracted so [`Builder::emit_heading`] stays focused on line assembly and
/// so future themes can override H4-H6 independently of H1-H3.
fn heading_color(level: HeadingLevel, p: &Palette) -> ratatui::style::Color {
    match level {
        HeadingLevel::H1 => p.h1,
        HeadingLevel::H2 => p.h2,
        HeadingLevel::H3 => p.h3,
        _ => p.heading_other,
    }
}

// ── Syntect highlighting helper ───────────────────────────────────────────────

/// Convert a syntect `Color` (RGBA) to a ratatui [`Color::Rgb`].
fn syntect_color_to_ratatui(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Return the first non-blank line of `s`, or `""` if all lines are blank.
///
/// Used by [`try_highlight_code`] to feed the first meaningful line to
/// syntect's shebang/pattern sniffer when the fenced block has no language tag.
fn first_non_blank_line(s: &str) -> &str {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("")
}

/// Resolve a GitHub-flavored-markdown language tag (e.g. `"rust"`,
/// `"typescript"`, `"bash"`) to a syntect `SyntaxReference`.
///
/// Syntect's `find_syntax_by_token` only matches against file extensions,
/// so the common long-form tags that people actually type in code fences
/// on GitHub (`"rust"` → extension is `"rs"`, `"python"` → `"py"`, and so
/// on) silently fail and land in the uncoloured plain-text fallback. This
/// helper adds a small alias table that covers the 95th percentile of tags
/// we've seen in real PR / issue comments.
fn resolve_syntax<'a>(
    ss: &'a SyntaxSet,
    tag: &str,
) -> Option<&'a syntect::parsing::SyntaxReference> {
    let normalized = tag.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }
    // 1. Direct match against file extensions (case-insensitive in syntect).
    if let Some(syntax) = ss.find_syntax_by_token(&normalized) {
        return Some(syntax);
    }
    // 2. Common long-form → extension aliases.
    let alias = match normalized.as_str() {
        "rust" => "rs",
        "python" => "py",
        "javascript" | "jsx" => "js",
        "typescript" | "tsx" => "ts",
        "ruby" => "rb",
        "golang" => "go",
        "kotlin" => "kt",
        "objective-c" | "objc" => "m",
        "shell" | "bash" | "zsh" | "ksh" => "sh",
        "c++" | "cxx" => "cpp",
        "c#" | "csharp" => "cs",
        "f#" | "fsharp" => "fs",
        "markdown" => "md",
        "yaml" => "yml",
        "dockerfile" => "Dockerfile",
        "html" => "html",
        "plaintext" | "text" | "txt" => return Some(ss.find_syntax_plain_text()),
        _ => return None,
    };
    ss.find_syntax_by_token(alias)
}

/// Attempt to syntax-highlight `source` for language `lang`.
///
/// Resolution order:
/// 1. Language tag via [`resolve_syntax`] (extension match + alias table).
/// 2. First-line sniffing (shebangs, distinctive opening lines).
/// 3. Plain Text fallback so the code background still applies uniformly.
///
/// Returns `None` when the block should use plain-colour rendering, either
/// because syntect cannot resolve the theme or because the source is too large
/// to highlight synchronously without risking an input/render stall.
fn try_highlight_code(
    source: &str,
    lang: &str,
    theme_name: &str,
    ss: &SyntaxSet,
    ts: &ThemeSet,
) -> Option<Vec<Line<'static>>> {
    if source.len() > MAX_SYNTAX_HIGHLIGHT_BYTES {
        return None;
    }

    let syntax = resolve_syntax(ss, lang)
        .or_else(|| ss.find_syntax_by_first_line(first_non_blank_line(source)))
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let theme = ts.themes.get(theme_name).or_else(|| ts.themes.get("base16-ocean.dark"))?;

    let mut h = HighlightLines::new(syntax, theme);
    let mut result = Vec::new();

    for line_str in LinesWithEndings::from(source) {
        let ranges = h.highlight_line(line_str, ss).ok()?;
        let spans: Vec<Span<'static>> = ranges
            .into_iter()
            .filter(|(_, text)| !text.trim_end_matches('\n').is_empty())
            .map(|(style, text)| {
                let fg = syntect_color_to_ratatui(style.foreground);
                let bg = syntect_color_to_ratatui(style.background);
                Span::styled(text.trim_end_matches('\n').to_owned(), Style::default().fg(fg).bg(bg))
            })
            .collect();
        result.push(Line::from(spans));
    }

    Some(result)
}

// ── Bounded comment rendering ─────────────────────────────────────────────────

/// Render a review/issue comment body.
///
/// Expanded comments render the complete Markdown body. Collapsed comments
/// render from a bounded source preview and then cap the rendered rows. This
/// keeps section switching and scroll clamping responsive even when a comment
/// contains a very large log, diff, table, or fenced code block.
pub(crate) fn render_comment_markdown(
    src: &str,
    palette: &Palette,
    expanded: bool,
) -> (Vec<Line<'static>>, bool) {
    if expanded {
        return (render_markdown(src, palette), false);
    }

    let (preview, source_truncated) =
        bounded_markdown_source(src, COMMENT_PREVIEW_SOURCE_LINES, COMMENT_PREVIEW_SOURCE_CHARS);
    let rendered = render_markdown(&preview, palette);
    let rendered_truncated = rendered.len() > COMMENT_PREVIEW_RENDERED_LINES;
    let visible = rendered.into_iter().take(COMMENT_PREVIEW_RENDERED_LINES).collect();

    (visible, source_truncated || rendered_truncated)
}

/// Copy at most `max_lines` source lines and `max_chars` Unicode scalar values.
///
/// The returned preview always ends at a valid UTF-8 boundary. Line limiting is
/// intentionally based on source lines rather than rendered rows, so large
/// fenced blocks are bounded before pulldown-cmark and syntect see them.
fn bounded_markdown_source(src: &str, max_lines: usize, max_chars: usize) -> (String, bool) {
    let mut out = String::new();
    let mut chars_used = 0usize;

    for (line_index, line) in src.split_inclusive('\n').enumerate() {
        if line_index >= max_lines {
            return (out, true);
        }

        for ch in line.chars() {
            if chars_used >= max_chars {
                return (out, true);
            }
            out.push(ch);
            chars_used += 1;
        }
    }

    (out, false)
}

// ── Event dispatcher ──────────────────────────────────────────────────────────

/// Handle a single pulldown-cmark event, mutating `b` in place.
///
/// Extracted from `render_markdown` to keep that function under the 100-line
/// pedantic limit.
#[allow(clippy::too_many_lines)]
fn handle_event(event: Event<'_>, b: &mut Builder<'_>, palette: &Palette) {
    match event {
        // ── Block-level opening tags ──────────────────────────────────────
        Event::Start(Tag::Heading { level, .. }) => {
            let (color, mods) = match level {
                HeadingLevel::H1 => (palette.h1, Modifier::BOLD),
                HeadingLevel::H2 => (palette.h2, Modifier::BOLD),
                HeadingLevel::H3 => (palette.h3, Modifier::empty()),
                _ => (palette.heading_other, Modifier::empty()),
            };
            b.push_style(|_| Style::default().fg(color).add_modifier(mods));
        }
        Event::End(TagEnd::Heading(level)) => {
            b.pop_style();
            b.emit_heading(level);
        }

        Event::End(TagEnd::Paragraph) => {
            b.flush_line();
            b.lines.push(Line::from(vec![]));
        }

        Event::Start(Tag::BlockQuote(_)) => {
            b.bq_depth += 1;
            b.push_style(|_| Style::default().fg(palette.block_quote_fg));
        }
        Event::End(TagEnd::BlockQuote(_)) => {
            b.flush_line();
            b.bq_depth = b.bq_depth.saturating_sub(1);
            b.pop_style();
        }

        Event::Start(Tag::CodeBlock(kind)) => {
            let lang = match kind {
                CodeBlockKind::Fenced(lang) => lang.to_string(),
                CodeBlockKind::Indented => String::new(),
            };
            b.code_block_lang = Some(lang);
        }
        Event::End(TagEnd::CodeBlock) => {
            b.emit_code_block();
        }

        // ── List handling ─────────────────────────────────────────────────
        Event::Start(Tag::List(start)) => {
            b.list_depth += 1;
            b.list_counter.push(start);
        }
        Event::End(TagEnd::List(_)) => {
            b.list_depth = b.list_depth.saturating_sub(1);
            b.list_counter.pop();
            if b.list_depth == 0 {
                b.lines.push(Line::from(vec![]));
            }
        }
        Event::Start(Tag::Item) => {
            // Prefix: `  • ` (unordered) or `  N. ` (ordered),
            // indented by 2 spaces per nesting level beyond the first.
            let indent = "  ".repeat(b.list_depth.saturating_sub(1));
            let prefix = match b.list_counter.last_mut() {
                Some(Some(n)) => {
                    let label = format!("{indent}  {n}. ");
                    *n += 1;
                    label
                }
                _ => format!("{indent}  \u{2022} "), // bullet: •
            };
            b.current_spans.push(InlineSpan::new(prefix, Style::default().fg(palette.list_marker)));
            // Restore foreground for item content.
            b.push_style(|_| Style::default().fg(palette.foreground));
        }
        Event::End(TagEnd::Item) => {
            b.pop_style();
            b.flush_line();
        }

        // ── Table (GFM) ───────────────────────────────────────────────────
        //
        // pulldown-cmark emits table events as:
        //   Start(Table(alignments))
        //     Start(TableHead)
        //       Start(TableCell) Text(...) End(TableCell) ...
        //     End(TableHead)
        //     Start(TableRow)
        //       Start(TableCell) Text(...) End(TableCell) ...
        //     End(TableRow)
        //     ...
        //   End(Table)
        //
        // We accumulate cell spans with the currently-active style stack so
        // bold / italic / inline-code inside cells render correctly, then
        // emit the whole table as bordered lines at `End(Table)`.
        Event::Start(Tag::Table(alignments)) => {
            b.in_table = true;
            b.table_alignments = alignments;
            b.table_header_row = None;
            b.table_body_rows.clear();
            b.table_current_row.clear();
            b.table_cell_spans.clear();
            b.table_in_header = false;
        }
        Event::Start(Tag::TableHead) => {
            b.table_in_header = true;
            b.table_current_row.clear();
        }
        Event::Start(Tag::TableRow) => {
            b.table_current_row.clear();
        }
        Event::Start(Tag::TableCell) => {
            b.table_cell_spans.clear();
        }
        Event::End(TagEnd::TableCell) => {
            let cell = std::mem::take(&mut b.table_cell_spans);
            b.table_current_row.push(cell);
        }
        Event::End(TagEnd::TableHead) => {
            b.table_header_row = Some(std::mem::take(&mut b.table_current_row));
            b.table_in_header = false;
        }
        Event::End(TagEnd::TableRow) => {
            if !b.table_in_header {
                b.table_body_rows.push(std::mem::take(&mut b.table_current_row));
            }
        }
        Event::End(TagEnd::Table) => {
            b.emit_table();
            b.in_table = false;
            b.lines.push(Line::from(vec![]));
        }

        // ── Inline style tags ─────────────────────────────────────────────
        Event::Start(Tag::Emphasis) => {
            b.push_style(|s| s.add_modifier(Modifier::ITALIC));
        }
        Event::Start(Tag::Strong) => {
            b.push_style(|s| s.add_modifier(Modifier::BOLD));
        }
        Event::Start(Tag::Strikethrough) => {
            b.push_style(|s| s.add_modifier(Modifier::CROSSED_OUT));
        }
        Event::Start(Tag::Link { .. }) => {
            // Display link text styled; URL is not inlined (too noisy).
            b.push_style(|_| Style::default().fg(palette.link).add_modifier(Modifier::UNDERLINED));
        }
        // All four inline closing tags pop one style level.
        Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link) => {
            b.pop_style();
        }

        // ── Inline code ───────────────────────────────────────────────────
        //
        // `Event::Code` has only two possible contexts (inline vs table cell)
        // because pulldown-cmark never emits a `Code` event inside a fenced
        // code block — those tokens arrive via `Event::Text` while
        // `code_block_lang.is_some()`. Cross-reference: `Event::Text`
        // dispatches via `TextSink` (three variants); if a third context is
        // ever needed here, update BOTH arms and extend `TextSink` accordingly.
        Event::Code(text) => {
            let style = Style::default().fg(palette.inline_code).bg(palette.code_bg);
            if b.in_table {
                b.table_cell_spans.push(Span::styled(text.to_string(), style));
            } else {
                b.current_spans.push(InlineSpan::new(text.to_string(), style));
            }
        }

        // ── Text content ──────────────────────────────────────────────────
        //
        // `Event::Text` dispatches via `TextSink` (three variants); `Event::Code`
        // has only two possible contexts (inline vs table cell) because inline
        // code events are never emitted inside a fenced code block. If a third
        // context is ever added to `Event::Code`, update BOTH arms.
        Event::Text(text) => {
            match b.text_sink() {
                TextSink::CodeBlock => {
                    b.code_block_buf.push_str(&text);
                }
                TextSink::TableCell => {
                    // Capture cell content with whatever style is active from
                    // Strong/Emphasis/Link/etc. wrapping tags.
                    b.table_cell_spans.push(Span::styled(text.to_string(), b.current_style()));
                }
                TextSink::Inline => {
                    b.push_text(&text);
                }
            }
        }

        // ── Line breaks ───────────────────────────────────────────────────
        Event::HardBreak | Event::SoftBreak => b.flush_line(),

        Event::Rule => {
            b.lines.push(Line::from(vec![Span::styled(
                "\u{2500}".repeat(40),
                Style::default().fg(palette.dim),
            )]));
        }

        // ── Task list checkbox (GFM) ──────────────────────────────────────
        Event::TaskListMarker(checked) => {
            let glyph = if checked { "[x] " } else { "[ ] " };
            b.current_spans.push(InlineSpan::new(glyph, Style::default().fg(palette.task_marker)));
        }

        // ── All remaining no-op tags ──────────────────────────────────────
        // Image: alt-text flows through Event::Text, tags are skipped.
        // Table-specific Start/End for Head/Row/Cell are handled above.
        // Footnotes, metadata, math, sub/superscript, definition lists:
        // either unsupported or handled via their child text events.
        Event::Html(_)
        | Event::InlineHtml(_)
        | Event::InlineMath(_)
        | Event::DisplayMath(_)
        | Event::FootnoteReference(_)
        | Event::Start(
            Tag::Paragraph
            | Tag::Image { .. }
            | Tag::HtmlBlock
            | Tag::Superscript
            | Tag::Subscript
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::FootnoteDefinition(_)
            | Tag::MetadataBlock(_),
        )
        | Event::End(
            TagEnd::Image
            | TagEnd::HtmlBlock
            | TagEnd::FootnoteDefinition
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::MetadataBlock(_),
        ) => {}
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Render `src` (GitHub-Flavored Markdown) into ratatui [`Line`]s.
///
/// The returned lines are `'static` — all string data is owned — so they can
/// be stored in application state or passed to a [`ratatui::widgets::Paragraph`]
/// without lifetime constraints.
///
/// # Arguments
///
/// * `src`     - Raw Markdown source string.
/// * `palette` - Active color palette; controls all foreground/background colors.
///
/// # Examples
///
/// ```
/// let palette = octopeek::theme::Palette::default();
/// let lines = octopeek::ui::markdown::render_markdown("# Hello\n\nworld", &palette);
/// assert!(!lines.is_empty());
/// ```
pub fn render_markdown(src: &str, palette: &Palette) -> Vec<Line<'static>> {
    let opts = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;

    let mut b = Builder::new(palette);
    for event in Parser::new_ext(src, opts) {
        handle_event(event, &mut b, palette);
    }

    // Flush any trailing inline content (source may not end with a newline).
    if !b.current_spans.is_empty() {
        b.flush_line();
    }

    // Strip trailing blank lines for a clean appearance.
    while b.lines.last().is_some_and(|l| l.spans.is_empty()) {
        b.lines.pop();
    }

    b.lines
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    fn palette() -> Palette {
        Palette::default()
    }

    /// Concatenate all span text in a line.
    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn plain_paragraph() {
        let lines = render_markdown("Hello, world.", &palette());
        let non_empty: Vec<_> = lines.iter().filter(|l| !l.spans.is_empty()).collect();
        assert_eq!(non_empty.len(), 1);
        assert_eq!(line_text(non_empty[0]), "Hello, world.");
    }

    #[test]
    fn headings_render_without_hash_prefix() {
        // Markdown hashes are syntax, not content. The renderer must strip
        // them and rely on bold/colour (plus rule lines for H1/H2) instead.
        let src = "# Title\n\n## Subtitle\n\n### Section\n";
        let p = palette();
        let lines = render_markdown(src, &p);
        let non_empty: Vec<_> = lines.iter().filter(|l| !l.spans.is_empty()).collect();

        // Find the content lines by their text (rule lines are pure `━`/`─`).
        let find_line = |needle: &str| {
            non_empty
                .iter()
                .find(|l| line_text(l).contains(needle))
                .copied()
                .unwrap_or_else(|| panic!("no line contains {needle:?}"))
        };
        let h1 = find_line("Title");
        let h2 = find_line("Subtitle");
        let h3 = find_line("Section");

        for (line, label) in [(h1, "h1"), (h2, "h2"), (h3, "h3")] {
            let text = line_text(line);
            assert!(!text.starts_with('#'), "{label} must not keep hash prefix: {text:?}");
            let main_span =
                line.spans.iter().find(|s| !s.content.trim().is_empty()).expect("heading span");
            assert!(
                main_span.style.add_modifier.contains(Modifier::BOLD),
                "{label} should be bold: {:?}",
                main_span.style
            );
        }

        assert_eq!(line_text(h1), "Title");
        assert_eq!(line_text(h2), "Subtitle");
        assert_eq!(line_text(h3), "Section");
    }

    #[test]
    fn h1_and_h2_emit_rule_underline() {
        let src = "# Title\n\n## Sub\n\n### NoRule\n";
        let p = palette();
        let lines = render_markdown(src, &p);
        let text_lines: Vec<String> = lines.iter().map(line_text).collect();

        let rule_h1 = text_lines.iter().find(|l| l.starts_with('\u{2501}'));
        let rule_h2 = text_lines.iter().find(|l| l.starts_with('\u{2500}'));
        assert!(rule_h1.is_some(), "h1 should emit a ━ rule line");
        assert!(rule_h2.is_some(), "h2 should emit a ─ rule line");

        // H3 must not get a rule.
        let after_h3 =
            text_lines.iter().position(|l| l.contains("NoRule")).expect("h3 line present");
        let next = text_lines.get(after_h3 + 1).map_or("", String::as_str);
        assert!(
            !next.starts_with('\u{2500}') && !next.starts_with('\u{2501}'),
            "h3 must not be followed by a rule; got {next:?}"
        );
    }

    #[test]
    fn collapsed_comment_markdown_bounds_source_preview() {
        let hidden = "HIDDEN_AFTER_PREVIEW_CAP";
        let repeated =
            (0..100).map(|i| format!("let value_{i} = {i};")).collect::<Vec<_>>().join("\n");
        let src = format!("```rust\n{repeated}\n```\n\n{hidden}");

        let (lines, truncated) = render_comment_markdown(&src, &palette(), false);
        let joined = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(truncated, "collapsed large comment must report truncation");
        assert!(
            lines.len() <= COMMENT_PREVIEW_RENDERED_LINES,
            "collapsed comment preview must cap rendered rows"
        );
        assert!(
            !joined.contains(hidden),
            "collapsed preview must not render content beyond the source cap"
        );
    }

    #[test]
    fn large_code_block_uses_plain_code_style() {
        let p = palette();
        let repeated = "let value = 1;\n".repeat((MAX_SYNTAX_HIGHLIGHT_BYTES / 14) + 8);
        let src = format!("```rust\n{repeated}```");

        let lines = render_markdown(&src, &p);
        let code_span = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.contains("let value"))
            .expect("large code block line");

        assert_eq!(code_span.style.fg, Some(p.code_fg));
        assert_eq!(code_span.style.bg, Some(p.code_bg));
    }

    #[test]
    fn bold_and_italic() {
        let src = "**bold** and *italic*";
        let lines = render_markdown(src, &palette());
        let all: String =
            lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(all.contains("bold") && all.contains("italic"), "missing text: {all}");

        let bold_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content == "bold")
            .expect("bold span");
        assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));

        let italic_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content == "italic")
            .expect("italic span");
        assert!(italic_span.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn inline_code() {
        let src = "Use `cargo test` to run tests.";
        let p = palette();
        let lines = render_markdown(src, &p);
        let span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("cargo test"))
            .expect("inline code span");
        assert_eq!(span.style.fg, Some(p.inline_code));
        assert_eq!(span.style.bg, Some(p.code_bg));
    }

    #[test]
    fn fenced_code_block_rust() {
        let src = "```rust\nfn main() {}\n```\n";
        let lines = render_markdown(src, &palette());
        let non_empty: Vec<_> = lines.iter().filter(|l| !l.spans.is_empty()).collect();
        assert!(!non_empty.is_empty(), "no lines from fenced block");
        assert!(
            non_empty.iter().any(|l| line_text(l).contains("main")),
            "fn main() not found in highlighted output"
        );
    }

    /// Regression test: `syntect::find_syntax_by_token` only matches file
    /// extensions, so common long-form GFM tags like `rust` / `python` /
    /// `javascript` used to silently drop into the Plain Text fallback.
    /// The alias table in `resolve_syntax` now maps them to the matching
    /// extension so real syntax highlighting kicks in and we see multiple
    /// distinct foreground colours across tokens within a single line.
    #[test]
    fn fenced_code_block_long_tag_produces_multicolor_spans() {
        let src = "```rust\nfn main() { let x: i32 = 42; println!(\"{}\", x); }\n```\n";
        let lines = render_markdown(src, &palette());
        let code_line =
            lines.iter().find(|l| line_text(l).contains("main")).expect("highlighted code line");

        // Collect the unique foreground colours seen across all spans on
        // the line. A syntax-highlighted line should have several — keyword,
        // identifier, type, literal, string — whereas Plain Text produces
        // exactly one colour across the whole line.
        let unique_fgs: std::collections::HashSet<_> =
            code_line.spans.iter().filter_map(|s| s.style.fg).collect();
        assert!(
            unique_fgs.len() >= 2,
            "expected multi-colour syntax highlighting, got {} distinct fg(s): {unique_fgs:?}",
            unique_fgs.len()
        );
    }

    /// Same regression guard for `python` (another common long-form tag
    /// whose extension is `py`, not `python`).
    #[test]
    fn fenced_code_block_python_long_tag_highlights() {
        let src = "```python\ndef f(x):\n    return x + 1\n```\n";
        let lines = render_markdown(src, &palette());
        let code_line =
            lines.iter().find(|l| line_text(l).contains("def")).expect("highlighted code line");
        let unique_fgs: std::collections::HashSet<_> =
            code_line.spans.iter().filter_map(|s| s.style.fg).collect();
        assert!(
            unique_fgs.len() >= 2,
            "python tag must resolve via alias table; got {unique_fgs:?}"
        );
    }

    #[test]
    fn fenced_code_block_untagged_falls_back_to_plain_text() {
        // No language tag — previously `try_highlight_code` returned `None`,
        // losing the code background. Now it sniffs the first line and falls
        // back to Plain Text so the block still renders with `code_bg`.
        let src = "```\nhello world\nsecond line\n```\n";
        let p = palette();
        let lines = render_markdown(src, &p);
        let non_empty: Vec<_> = lines.iter().filter(|l| !l.spans.is_empty()).collect();
        let text: String = non_empty.iter().map(|l| line_text(l)).collect::<Vec<_>>().join("\n");
        assert!(text.contains("hello world"), "missing first line: {text}");
        assert!(text.contains("second line"), "missing second line: {text}");
    }

    #[test]
    fn fenced_code_block_untagged_with_shebang_detected() {
        // A shebang on the first line lets syntect's pattern sniffer pick a
        // real syntax even without a language tag.
        let src = "```\n#!/bin/bash\necho hi\n```\n";
        let lines = render_markdown(src, &palette());
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("#!/bin/bash"), "shebang missing: {text}");
        assert!(text.contains("echo hi"), "body missing: {text}");
    }

    #[test]
    fn bullet_and_ordered_lists() {
        let src = "- apple\n- banana\n\n1. first\n2. second\n";
        let lines = render_markdown(src, &palette());
        let text: String =
            lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(text.contains('\u{2022}'), "bullet missing: {text}");
        assert!(text.contains("apple") && text.contains("banana"), "items: {text}");
        assert!(text.contains("1.") && text.contains("2."), "ordered: {text}");
        assert!(text.contains("first") && text.contains("second"), "values: {text}");
    }

    #[test]
    fn link_styled_with_link_color() {
        let src = "[GitHub](https://github.com)";
        let p = palette();
        let lines = render_markdown(src, &p);
        let span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("GitHub"))
            .expect("link span");
        assert_eq!(span.style.fg, Some(p.link));
        assert!(span.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn blockquote_has_border_prefix() {
        let src = "> This is a quote.";
        let p = palette();
        let lines = render_markdown(src, &p);
        let bq = lines
            .iter()
            .find(|l| line_text(l).contains("This is a quote"))
            .expect("blockquote line");
        let border = bq.spans.first().expect("no spans");
        // ▌ = U+258C
        assert!(border.content.contains('\u{258c}'), "border glyph missing: {:?}", border.content);
        assert_eq!(border.style.fg, Some(p.block_quote_border));
    }

    #[test]
    fn table_renders_headers_and_rows_as_bordered_lines() {
        // Minimum viable GFM table — two columns, one header, two body rows.
        let src = "| Col A | Col B |\n|---|---|\n| a1 | b1 |\n| a2 | b2 |\n";
        let lines = render_markdown(src, &palette());
        let text_lines: Vec<String> = lines.iter().map(line_text).collect();
        let joined = text_lines.join("\n");

        // Content must be preserved across all four cells plus both headers.
        for needle in ["Col A", "Col B", "a1", "b1", "a2", "b2"] {
            assert!(joined.contains(needle), "missing cell {needle:?} in: {joined}");
        }

        // At least one line must use the heavy box-drawing vertical bar; the
        // top/bottom borders use `─` (U+2500) so their presence confirms the
        // table is rendered as a bordered block rather than the old
        // `[table]` placeholder.
        assert!(
            joined.contains('\u{2502}'),
            "vertical border │ missing — table not rendered as bordered block: {joined}"
        );
        assert!(
            joined.contains('\u{250C}') && joined.contains('\u{2518}'),
            "corner borders ┌ / ┘ missing: {joined}"
        );

        // Explicitly verify the old placeholder is gone so a future
        // regression to "[table]" fails loudly.
        assert!(!joined.contains("[table]"), "table placeholder leaked back into output: {joined}");
    }

    #[test]
    fn table_preserves_inline_cell_styling() {
        // Bold + inline code inside a cell must keep their respective styles
        // when the cell is rendered inside the bordered table.
        let src = "| Before | Styled |\n|---|---|\n| **bold** | `code` |\n";
        let p = palette();
        let lines = render_markdown(src, &p);
        let has_bold = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.content == "bold" && s.style.add_modifier.contains(Modifier::BOLD));
        assert!(has_bold, "bold cell content lost its BOLD modifier");

        let has_code = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.content == "code" && s.style.bg == Some(p.code_bg));
        assert!(has_code, "inline-code cell lost its code_bg style");
    }

    // ── TextSink snapshot tests (Step 1) ──────────────────────────────────────
    //
    // These lock current behaviour for the three text-routing contexts before
    // the TextSink enum refactor is applied. Assertions are aggregate/structural
    // (not literal Vec<Line> comparisons) so they stay green across minor
    // palette tweaks while still catching routing regressions.

    /// Snapshot: inline paragraph with bold, inline code, and a link.
    ///
    /// Verifies that each inline context (plain text, bold span, inline code,
    /// link) produces the expected spans and styles via the existing if-chain.
    #[test]
    fn text_sink_snapshot_inline_paragraph() {
        let src = "Normal **bold** `code` and [link](https://example.com) text.";
        let p = palette();
        let lines = render_markdown(src, &p);

        // The whole paragraph is one logical line (no hard breaks).
        let non_empty: Vec<_> = lines.iter().filter(|l| !l.spans.is_empty()).collect();
        assert_eq!(
            non_empty.len(),
            1,
            "expected exactly 1 non-empty line, got {}",
            non_empty.len()
        );

        // All span text concatenated must contain every input word.
        let all_text: String = non_empty[0].spans.iter().map(|s| s.content.as_ref()).collect();
        for word in ["Normal", "bold", "code", "link", "text"] {
            assert!(all_text.contains(word), "missing word {word:?} in: {all_text:?}");
        }

        // At least one bold span.
        let bold_count = non_empty[0]
            .spans
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::BOLD))
            .count();
        assert!(bold_count >= 1, "expected at least 1 bold span, got {bold_count}");

        // Exactly one inline-code span (palette.inline_code fg).
        let inline_code_count =
            non_empty[0].spans.iter().filter(|s| s.style.fg == Some(p.inline_code)).count();
        assert_eq!(inline_code_count, 1, "expected 1 inline-code span, got {inline_code_count}");
    }

    /// Snapshot: triple-backtick-fenced Rust block with 3 source lines.
    ///
    /// Verifies that (a) the rendered line count matches the source lines,
    /// (b) every content span carries a non-None fg (syntect applied colour),
    /// and (c) the raw language tag "rust" does not leak into rendered text.
    #[test]
    fn text_sink_snapshot_fenced_code_block() {
        let src = "```rust\nlet x = 1;\nlet y = 2;\nlet z = x + y;\n```\n";
        let lines = render_markdown(src, &palette());

        // Three source lines → three non-empty rendered lines
        // (blank lines between/after blocks are also emitted, so filter them).
        let content_lines: Vec<_> = lines.iter().filter(|l| !l.spans.is_empty()).collect();
        assert_eq!(
            content_lines.len(),
            3,
            "expected 3 code content lines, got {}; lines: {content_lines:?}",
            content_lines.len()
        );

        // Every span on code-content lines must have a fg colour (syntect was applied).
        for (i, line) in content_lines.iter().enumerate() {
            for span in &line.spans {
                assert!(
                    span.style.fg.is_some(),
                    "code line {i} has a span with no fg colour: {span:?}"
                );
            }
        }

        // The raw language tag must not appear verbatim in the rendered output.
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        // "rust" might appear inside code (e.g. variable names), but the tag
        // line itself renders as nothing — there should be no standalone "rust"
        // on a line by itself (the lang line is consumed, not emitted).
        let rust_only_line = lines.iter().any(|l| {
            let t: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            t.trim() == "rust"
        });
        assert!(
            !rust_only_line,
            "language tag 'rust' leaked as a standalone line in: {all_text:?}"
        );
    }

    /// Snapshot: 2-column, 3-row GFM table (header + 2 body rows).
    ///
    /// Verifies border characters, cell content preservation, and that the
    /// rendered row count equals header + separator + data rows + 2 border rows.
    #[test]
    fn text_sink_snapshot_gfm_table() {
        // 2 columns, 1 header row, 2 body rows → 3 data rows total.
        let src = "| Name | Value |\n|---|---|\n| alpha | 1 |\n| beta | 2 |\n";
        let lines = render_markdown(src, &palette());

        let text_lines: Vec<String> = lines.iter().map(line_text).collect();
        let joined = text_lines.join("\n");

        // Cell content must be preserved.
        for word in ["Name", "Value", "alpha", "1", "beta", "2"] {
            assert!(joined.contains(word), "missing cell content {word:?} in:\n{joined}");
        }

        // Box-drawing border characters must be present.
        assert!(
            joined.contains('\u{250C}') || joined.contains('+'),
            "top-left corner (┌ or +) missing from table output:\n{joined}"
        );

        // Non-empty rendered lines: top border + header + separator +
        // 2 body rows + bottom border = 6. (Blank line after table is empty.)
        let non_empty_count = lines.iter().filter(|l| !l.spans.is_empty()).count();
        // rows=2, plus header=1, plus 3 border lines (top/sep/bottom) = 6.
        assert_eq!(
            non_empty_count, 6,
            "expected 6 non-empty lines (borders+header+sep+2 rows), got {non_empty_count};\n{joined}"
        );
    }
}
