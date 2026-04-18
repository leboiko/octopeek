//! Tmux-style "copy mode" for the PR / issue detail views.
//!
//! When active, line wrapping is disabled and a block cursor appears over the
//! content. The user can move the cursor with `h`/`j`/`k`/`l` (or arrow keys),
//! toggle a selection anchor with `V`, and yank the selected text to the
//! system clipboard with `y`. `Esc` exits without copying.
//!
//! State is intentionally absolute (row/col offsets into the rendered
//! `Vec<Line>`) rather than screen-relative, so scrolling the viewport does
//! not move the cursor in logical space.
//!
//! Cursor column is measured in *characters*, not display cells. Display
//! positioning accounts for unicode width when rendering and extracting.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::theme::Palette;

/// A (row, col) coordinate into a `Vec<Line>`. `col` counts characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub row: usize,
    pub col: usize,
}

/// Copy-mode state owned by `App`. Inactive by default.
#[derive(Debug, Clone, Default)]
pub struct CopyMode {
    /// `true` while the mode is active. Normal key bindings are suppressed.
    pub active: bool,
    /// Current cursor position.
    pub cursor: Pos,
    /// When `Some`, the cursor/anchor pair defines an inclusive selection.
    pub anchor: Option<Pos>,
    /// Horizontal scroll offset (columns) for the underlying paragraph.
    pub h_scroll: u16,
}

impl CopyMode {
    /// Enter copy mode with the cursor at `(row, col)` and no active selection.
    pub fn enter(&mut self, row: usize, col: usize) {
        self.active = true;
        self.cursor = Pos { row, col };
        self.anchor = None;
        self.h_scroll = 0;
    }

    /// Exit copy mode, clearing cursor/anchor state.
    pub fn exit(&mut self) {
        self.active = false;
        self.cursor = Pos::default();
        self.anchor = None;
        self.h_scroll = 0;
    }

    /// Start (or clear) a selection anchored at the current cursor position.
    pub fn toggle_selection(&mut self) {
        if self.anchor.is_some() {
            self.anchor = None;
        } else {
            self.anchor = Some(self.cursor);
        }
    }

    /// Move cursor by `(dx, dy)`, clamped to `lines` bounds. `dx` is in chars.
    pub fn move_cursor(&mut self, dx: i32, dy: i32, lines: &[Line<'_>]) {
        if lines.is_empty() {
            return;
        }
        let last_row = lines.len().saturating_sub(1);
        let new_row = clamp_add(self.cursor.row, dy, last_row);
        let row_len = line_char_len(&lines[new_row]);
        let new_col = clamp_add(self.cursor.col, dx, row_len);
        self.cursor = Pos { row: new_row, col: new_col };
    }

    /// Jump cursor to the first row (`gg`).
    pub fn jump_top(&mut self) {
        self.cursor.row = 0;
        self.cursor.col = 0;
    }

    /// Jump cursor to the last row (`G`).
    pub fn jump_bottom(&mut self, lines: &[Line<'_>]) {
        if lines.is_empty() {
            return;
        }
        self.cursor.row = lines.len().saturating_sub(1);
        self.cursor.col = 0;
    }

    /// Return the selected text as a single `String`, or `None` when no
    /// selection is active. Lines are joined with `\n`.
    pub fn selected_text(&self, lines: &[Line<'_>]) -> Option<String> {
        let anchor = self.anchor?;
        let (start, end) = ordered(anchor, self.cursor);
        let mut out = String::new();
        for row in start.row..=end.row {
            if row >= lines.len() {
                break;
            }
            let line_text = line_to_string(&lines[row]);
            let chars: Vec<char> = line_text.chars().collect();

            let from = if row == start.row { start.col } else { 0 };
            let to = if row == end.row { end.col.saturating_add(1) } else { chars.len() };
            let to = to.min(chars.len());
            let from = from.min(to);

            if row > start.row {
                out.push('\n');
            }
            out.extend(&chars[from..to]);
        }
        Some(out)
    }
}

/// Produce a new set of lines with selection background and cursor cell
/// highlighting applied. The input is not modified in place because spans may
/// need to be split to highlight a single cell.
pub fn apply_overlay(lines: &[Line<'static>], cm: &CopyMode, p: &Palette) -> Vec<Line<'static>> {
    if !cm.active {
        return lines.to_vec();
    }
    let sel = cm.anchor.map(|a| ordered(a, cm.cursor));
    let cursor_style =
        Style::default().bg(p.selection_bg).fg(p.selection_fg).add_modifier(Modifier::REVERSED);
    let sel_style = Style::default().bg(p.selection_bg).fg(p.selection_fg);

    lines
        .iter()
        .enumerate()
        .map(|(row, line)| {
            let sel_range = sel.and_then(|(s, e)| row_sel_range(row, s, e, line_char_len(line)));
            let cursor_col = if row == cm.cursor.row { Some(cm.cursor.col) } else { None };
            overlay_line(line, sel_range, cursor_col, sel_style, cursor_style)
        })
        .collect()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn clamp_add(value: usize, delta: i32, max_inclusive: usize) -> usize {
    let v = i64::try_from(value).unwrap_or(i64::MAX);
    let new = v.saturating_add(i64::from(delta));
    let max = i64::try_from(max_inclusive).unwrap_or(i64::MAX);
    new.clamp(0, max).try_into().unwrap_or(0)
}

fn line_char_len(line: &Line<'_>) -> usize {
    line.spans.iter().map(|s| s.content.chars().count()).sum()
}

fn line_to_string(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

/// Ensure `a` is the earlier position and `b` the later, in row-major order.
fn ordered(a: Pos, b: Pos) -> (Pos, Pos) {
    if (a.row, a.col) <= (b.row, b.col) { (a, b) } else { (b, a) }
}

/// Return the `[from, to]` inclusive char-column range of the selection on
/// `row`, clamped to `row_len`. Returns `None` when the row is not in range.
fn row_sel_range(row: usize, start: Pos, end: Pos, row_len: usize) -> Option<(usize, usize)> {
    if row < start.row || row > end.row {
        return None;
    }
    let from = if row == start.row { start.col } else { 0 };
    let to = if row == end.row { end.col } else { row_len };
    Some((from, to.min(row_len)))
}

/// Rebuild a single line with per-character styling applied to the selection
/// range and cursor column.
fn overlay_line(
    line: &Line<'static>,
    sel_range: Option<(usize, usize)>,
    cursor_col: Option<usize>,
    sel_style: Style,
    cursor_style: Style,
) -> Line<'static> {
    // Handle empty lines: render a one-cell cursor spacer so the block is visible.
    if line.spans.is_empty() {
        if cursor_col == Some(0) {
            return Line::from(vec![Span::styled(" ", cursor_style)]);
        }
        return line.clone();
    }

    let mut out: Vec<Span<'static>> = Vec::with_capacity(line.spans.len());
    let mut col: usize = 0;
    for span in &line.spans {
        let base = span.style;
        let mut buf = String::new();
        let mut buf_style = base;
        for ch in span.content.chars() {
            let is_cursor = cursor_col == Some(col);
            let is_selected = sel_range.is_some_and(|(f, t)| col >= f && col <= t);
            let style = if is_cursor {
                cursor_style
            } else if is_selected {
                merge(base, sel_style)
            } else {
                base
            };
            if style != buf_style && !buf.is_empty() {
                out.push(Span::styled(std::mem::take(&mut buf), buf_style));
            }
            buf_style = style;
            buf.push(ch);
            col += 1;
        }
        if !buf.is_empty() {
            out.push(Span::styled(buf, buf_style));
        }
    }

    // Cursor past end of line (e.g. cursor at col == row_len): append a spacer.
    if cursor_col == Some(col) {
        out.push(Span::styled(" ".to_owned(), cursor_style));
    }
    Line::from(out)
}

/// Merge `overlay` on top of `base`, preferring overlay's fg/bg when set.
fn merge(base: Style, overlay: Style) -> Style {
    Style {
        fg: overlay.fg.or(base.fg),
        bg: overlay.bg.or(base.bg),
        add_modifier: base.add_modifier | overlay.add_modifier,
        sub_modifier: base.sub_modifier | overlay.sub_modifier,
        underline_color: overlay.underline_color.or(base.underline_color),
    }
}

/// Display-column offset of char index `col` within `line`. Used by callers
/// that need to adjust horizontal scroll to keep the cursor visible.
#[must_use]
pub fn cursor_display_col(line: &Line<'_>, col: usize) -> u16 {
    let mut chars_left = col;
    let mut display = 0usize;
    for span in &line.spans {
        for ch in span.content.chars() {
            if chars_left == 0 {
                return u16::try_from(display).unwrap_or(u16::MAX);
            }
            display += UnicodeWidthStr::width(ch.to_string().as_str());
            chars_left -= 1;
        }
    }
    u16::try_from(display).unwrap_or(u16::MAX)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn lines(texts: &[&str]) -> Vec<Line<'static>> {
        texts.iter().map(|t| Line::from(vec![Span::raw((*t).to_owned())])).collect()
    }

    fn pal() -> Palette {
        Palette::default()
    }

    #[test]
    fn enter_and_exit() {
        let mut cm = CopyMode::default();
        assert!(!cm.active);
        cm.enter(3, 5);
        assert!(cm.active);
        assert_eq!(cm.cursor, Pos { row: 3, col: 5 });
        assert!(cm.anchor.is_none());
        cm.exit();
        assert!(!cm.active);
    }

    #[test]
    fn move_cursor_clamps_to_bounds() {
        let ls = lines(&["hello", "world!!"]);
        let mut cm = CopyMode::default();
        cm.enter(0, 0);

        cm.move_cursor(100, 0, &ls);
        assert_eq!(cm.cursor, Pos { row: 0, col: 5 }, "clamp to row len");

        cm.move_cursor(0, 100, &ls);
        assert_eq!(cm.cursor.row, 1, "clamp to last row");
        // Col clamps against new row length (7).
        assert_eq!(cm.cursor.col, 5);

        cm.move_cursor(-100, -100, &ls);
        assert_eq!(cm.cursor, Pos { row: 0, col: 0 });
    }

    #[test]
    fn selection_single_line() {
        let ls = lines(&["the quick brown fox"]);
        let mut cm = CopyMode::default();
        cm.enter(0, 4);
        cm.toggle_selection();
        cm.move_cursor(8, 0, &ls); // col 4..=12 -> "quick bro"
        let sel = cm.selected_text(&ls).expect("selection");
        assert_eq!(sel, "quick bro");
    }

    #[test]
    fn selection_multi_line_joins_with_newline() {
        let ls = lines(&["abc", "def", "ghi"]);
        let mut cm = CopyMode::default();
        cm.enter(0, 1);
        cm.toggle_selection();
        // anchor (0,1) + dx=1, dy=2 -> cursor (2,2); inclusive selection.
        cm.move_cursor(1, 2, &ls);
        let sel = cm.selected_text(&ls).expect("selection");
        assert_eq!(sel, "bc\ndef\nghi");
    }

    #[test]
    fn selection_handles_reversed_direction() {
        let ls = lines(&["hello world"]);
        let mut cm = CopyMode::default();
        cm.enter(0, 8);
        cm.toggle_selection();
        // cursor moves before anchor; selection is inclusive on both ends.
        cm.move_cursor(-6, 0, &ls);
        let sel = cm.selected_text(&ls).expect("selection");
        assert_eq!(sel, "llo wor");
    }

    #[test]
    fn selected_text_none_when_no_anchor() {
        let ls = lines(&["hi"]);
        let mut cm = CopyMode::default();
        cm.enter(0, 0);
        cm.move_cursor(1, 0, &ls);
        assert!(cm.selected_text(&ls).is_none());
    }

    #[test]
    fn toggle_selection_clears_when_active() {
        let mut cm = CopyMode::default();
        cm.enter(0, 0);
        cm.toggle_selection();
        assert!(cm.anchor.is_some());
        cm.toggle_selection();
        assert!(cm.anchor.is_none());
    }

    #[test]
    fn apply_overlay_marks_cursor_cell() {
        let ls = lines(&["abc"]);
        let mut cm = CopyMode::default();
        cm.enter(0, 1);
        let out = apply_overlay(&ls, &cm, &pal());
        let txt: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(txt, "abc");
        let cursor_span = out[0].spans.iter().find(|s| s.content == "b").expect("cursor span");
        assert!(cursor_span.style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn apply_overlay_paints_selection_bg() {
        let ls = lines(&["abcdef"]);
        let mut cm = CopyMode::default();
        cm.enter(0, 1);
        cm.toggle_selection();
        cm.move_cursor(2, 0, &ls);
        let out = apply_overlay(&ls, &cm, &pal());
        // Chars at col 1..=3 ("bcd") should have selection bg.
        let joined: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "abcdef");
        let has_selected_bg = out[0].spans.iter().any(|s| s.style.bg == Some(pal().selection_bg));
        assert!(has_selected_bg, "no span carries selection_bg: {out:?}");
    }

    #[test]
    fn apply_overlay_skips_when_inactive() {
        let ls = lines(&["abc"]);
        let cm = CopyMode::default();
        let out = apply_overlay(&ls, &cm, &pal());
        assert_eq!(out.len(), 1);
        // No cursor span with REVERSED modifier.
        assert!(!out[0].spans.iter().any(|s| s.style.add_modifier.contains(Modifier::REVERSED)));
    }

    #[test]
    fn cursor_past_end_appends_spacer() {
        let ls = lines(&["ab"]);
        let mut cm = CopyMode::default();
        cm.enter(0, 2); // just past the last char — would be clamped by move_cursor
        let out = apply_overlay(&ls, &cm, &pal());
        let joined: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "ab ");
    }

    #[test]
    fn cursor_on_empty_line_renders_spacer() {
        let ls: Vec<Line<'static>> = vec![Line::from(vec![])];
        let mut cm = CopyMode::default();
        cm.enter(0, 0);
        let out = apply_overlay(&ls, &cm, &pal());
        assert_eq!(out[0].spans.len(), 1);
        assert_eq!(out[0].spans[0].content, " ");
    }

    #[test]
    fn cursor_display_col_counts_unicode_width() {
        let line = Line::from(vec![Span::raw("aé漢b")]);
        // a=1, é=1, 漢=2
        assert_eq!(cursor_display_col(&line, 0), 0);
        assert_eq!(cursor_display_col(&line, 1), 1);
        assert_eq!(cursor_display_col(&line, 2), 2);
        assert_eq!(cursor_display_col(&line, 3), 4);
    }

    #[test]
    fn selection_does_not_replace_fg_when_base_is_styled() {
        // A span with an existing fg should keep the selection bg overlaid
        // but the fg preference is overlay's (selection_fg).
        let ls: Vec<Line<'static>> =
            vec![Line::from(vec![Span::styled("xy".to_owned(), Style::default().fg(Color::Red))])];
        let mut cm = CopyMode::default();
        cm.enter(0, 0);
        cm.toggle_selection();
        cm.move_cursor(1, 0, &ls);
        let out = apply_overlay(&ls, &cm, &pal());
        assert!(out[0].spans.iter().any(|s| s.style.bg == Some(pal().selection_bg)));
    }
}
