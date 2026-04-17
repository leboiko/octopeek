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

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
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

// ── Internal builder types ────────────────────────────────────────────────────

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
    /// `true` while rendering inside a table (we emit a placeholder once).
    in_table: bool,
    /// `true` once we have already emitted the `[table]` placeholder for the
    /// current table block, so we do not repeat it for every cell event.
    table_placeholder_emitted: bool,
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
            table_placeholder_emitted: false,
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

/// Attempt to syntax-highlight `source` for language `lang`.
///
/// When `lang` is empty, tries to auto-detect the language by feeding the
/// first non-blank line to syntect's pattern sniffer (shebangs, keywords).
/// If detection still fails, falls back to the "Plain Text" syntax so the
/// code background is applied uniformly even without per-token colours.
///
/// Returns `None` only when syntect's theme cannot be resolved, so the
/// caller can fall back to plain-colour rendering.
fn try_highlight_code(
    source: &str,
    lang: &str,
    theme_name: &str,
    ss: &SyntaxSet,
    ts: &ThemeSet,
) -> Option<Vec<Line<'static>>> {
    let syntax = if lang.is_empty() {
        // 1. Try sniffing via the first non-blank line (shebangs, etc.).
        let first = first_non_blank_line(source);
        ss.find_syntax_by_first_line(first)
            // 2. Fall back to Plain Text so code_bg is still applied.
            .or_else(|| ss.find_syntax_plain_text().into())
    } else {
        ss.find_syntax_by_token(lang)
    }?;

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
        Event::Start(Tag::Table(_)) => {
            b.in_table = true;
            b.table_placeholder_emitted = false;
        }
        Event::End(TagEnd::Table) => {
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
        Event::Code(text) => {
            b.current_spans.push(InlineSpan::new(
                text.to_string(),
                Style::default().fg(palette.inline_code).bg(palette.code_bg),
            ));
        }

        // ── Text content ──────────────────────────────────────────────────
        Event::Text(text) => {
            if b.code_block_lang.is_some() {
                b.code_block_buf.push_str(&text);
            } else if b.in_table && !b.table_placeholder_emitted {
                b.lines.push(Line::from(vec![Span::styled(
                    "[table]",
                    Style::default().fg(palette.dim),
                )]));
                b.table_placeholder_emitted = true;
            } else if !b.in_table {
                b.push_text(&text);
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
        // Table sub-tags: table content is captured by Event::Text above.
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
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
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
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::TableCell
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
            let main_span = line
                .spans
                .iter()
                .find(|s| !s.content.trim().is_empty())
                .expect("heading span");
            assert!(
                main_span.style.add_modifier.contains(Modifier::BOLD),
                "{label} should be bold: {:?}", main_span.style
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
        let after_h3 = text_lines
            .iter()
            .position(|l| l.contains("NoRule"))
            .expect("h3 line present");
        let next = text_lines.get(after_h3 + 1).map_or("", String::as_str);
        assert!(
            !next.starts_with('\u{2500}') && !next.starts_with('\u{2501}'),
            "h3 must not be followed by a rule; got {next:?}"
        );
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
    fn table_emits_placeholder() {
        let src = "| A | B |\n|---|---|\n| 1 | 2 |\n";
        let lines = render_markdown(src, &palette());
        let text: String =
            lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(text.contains("[table]"), "table placeholder missing: {text}");
    }
}
