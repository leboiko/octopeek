//! Sticky header, banner line, and copy-mode tint helpers.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::github::detail::PrDetail;
use crate::theme::Palette;
use crate::ui::util::humanize_delta;

use super::checks::check_is_failing;

/// Short state label + color for the sticky header's top line.
fn pr_state_label(detail: &PrDetail, p: &Palette) -> (&'static str, Color) {
    if detail.merged {
        ("MERGED", p.accent_alt)
    } else if detail.is_draft {
        ("DRAFT", p.dim)
    } else {
        ("OPEN", p.success)
    }
}

/// Build the sticky header lines for a PR.
///
/// The header is rendered in its own fixed-height region above the scrolling
/// body so the reader never loses the repo/number/title/stats context. Returns
/// one `Line` per visible row; callers use `len()` for layout sizing.
pub fn build_header(detail: &PrDetail, p: &Palette) -> Vec<Line<'static>> {
    let (state_text, state_color) = pr_state_label(detail, p);
    let age = humanize_delta(&detail.created_at);

    // Line 1: repo + number + state + author + age.
    let line1 = Line::from(vec![
        Span::styled(
            format!("{} #{}", detail.repo, detail.number),
            Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  \u{00B7}  ", Style::default().fg(p.dim)),
        Span::styled(state_text, Style::default().fg(state_color).add_modifier(Modifier::BOLD)),
        Span::styled("  \u{00B7}  ", Style::default().fg(p.dim)),
        Span::styled(format!("@{}", detail.author), Style::default().fg(p.foreground)),
        Span::styled(format!(" opened {age}"), Style::default().fg(p.dim)),
    ]);

    // Line 2: title.
    let line2 = Line::from(Span::styled(
        detail.title.clone(),
        Style::default().fg(p.foreground).add_modifier(Modifier::BOLD),
    ));

    // Line 3: branches + diff stats + comment count.
    let line3 = Line::from(vec![
        Span::styled(detail.head_ref.clone(), Style::default().fg(p.accent_alt)),
        Span::styled(" \u{2192} ", Style::default().fg(p.dim)), // →
        Span::styled(detail.base_ref.clone(), Style::default().fg(p.accent_alt)),
        Span::styled("  \u{00B7}  ", Style::default().fg(p.dim)),
        Span::styled(format!("+{}", detail.additions), Style::default().fg(p.git_new)),
        Span::styled(" ", Style::default()),
        Span::styled(
            format!("\u{2212}{}", detail.deletions), // −
            Style::default().fg(p.danger),
        ),
        Span::styled(
            format!(
                "  across {} files  \u{00B7}  {} comments",
                detail.changed_files_count,
                detail.issue_comments.len()
            ),
            Style::default().fg(p.dim),
        ),
    ]);

    let mut header = vec![line1, line2, line3];
    if let Some(banner) = banner_line(detail, p) {
        header.push(banner);
    }
    header
}

/// Produce the flag banner line (may be empty) for the top of the detail view.
fn banner_line(detail: &PrDetail, p: &Palette) -> Option<Line<'static>> {
    if detail.is_draft {
        return None;
    }
    if detail.merged {
        return None;
    }
    let has_failing = detail.check_runs.iter().any(check_is_failing);
    if has_failing {
        return Some(Line::from(Span::styled(
            "\u{2716} CI FAILING".to_owned(),
            Style::default().fg(p.danger).add_modifier(Modifier::BOLD),
        )));
    }
    None
}

// ── Copy-mode tint helpers ────────────────────────────────────────────────────

/// Apply the tint to every span of `line` and right-pad to `width` cells.
///
/// Used in copy mode where line-wrap is disabled and the logical line maps
/// one-to-one to a screen row.
pub(super) fn tint_line(line: &Line<'static>, bg: Color, width: u16) -> Line<'static> {
    let current_width: usize =
        line.spans.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
    let target = usize::from(width);
    let pad = target.saturating_sub(current_width);

    let mut spans: Vec<Span<'static>> =
        line.spans.iter().map(|s| Span::styled(s.content.clone(), s.style.bg(bg))).collect();
    if pad > 0 {
        spans.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
    }
    let mut result = Line::from(spans);
    result.style = Style::default().bg(bg);
    result
}

/// Pre-wrap `line` at character boundaries into one-or-more `Line`s, each
/// exactly `width` cells wide and fully tinted with `bg`.
///
/// Uses display width (unicode-width) so CJK and emoji don't blow through
/// the column budget.
pub(super) fn char_wrap_tint(line: &Line<'static>, bg: Color, width: u16) -> Vec<Line<'static>> {
    let w = usize::from(width).max(1);
    let bg_style = Style::default().bg(bg);

    let mut out: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut current_w = 0usize;

    for span in &line.spans {
        let tinted = span.style.bg(bg);
        let span_w = UnicodeWidthStr::width(span.content.as_ref());
        if current_w + span_w <= w {
            current.push(Span::styled(span.content.clone(), tinted));
            current_w += span_w;
            continue;
        }
        // Slow path: walk the span char-by-char, splitting at the column boundary.
        let mut buf = String::new();
        let mut buf_w = 0usize;
        for ch in span.content.chars() {
            let cw = UnicodeWidthStr::width(ch.to_string().as_str()).max(1);
            if current_w + buf_w + cw > w {
                if !buf.is_empty() {
                    current.push(Span::styled(std::mem::take(&mut buf), tinted));
                    current_w += buf_w;
                    buf_w = 0;
                }
                flush_tinted_line(&mut current, current_w, w, bg_style, &mut out);
                current_w = 0;
            }
            buf.push(ch);
            buf_w += cw;
        }
        if !buf.is_empty() {
            current.push(Span::styled(buf, tinted));
            current_w += buf_w;
        }
    }

    flush_tinted_line(&mut current, current_w, w, bg_style, &mut out);
    out
}

/// Push `current` as a finished, `width`-cell tinted line and reset.
/// Factored out to keep [`char_wrap_tint`] under the pedantic line limit.
pub(super) fn flush_tinted_line(
    current: &mut Vec<Span<'static>>,
    current_w: usize,
    width: usize,
    bg_style: Style,
    out: &mut Vec<Line<'static>>,
) {
    let pad = width.saturating_sub(current_w);
    if pad > 0 {
        current.push(Span::styled(" ".repeat(pad), bg_style));
    }
    let mut line = Line::from(std::mem::take(current));
    line.style = bg_style;
    out.push(line);
}

/// Return a copy of `lines` with `alt_bg` applied to every line whose index
/// falls within any `(start, end)` half-open range in `alt_ranges`.
pub(super) fn apply_alt_bg(
    lines: &[Line<'static>],
    alt_ranges: &[(u16, u16)],
    bg: Color,
    width: u16,
    wrap_enabled: bool,
) -> Vec<Line<'static>> {
    if alt_ranges.is_empty() || width == 0 {
        return lines.to_vec();
    }
    let mut out: Vec<Line<'static>> = Vec::with_capacity(lines.len());
    for (idx, line) in lines.iter().enumerate() {
        let in_range = alt_ranges.iter().any(|&(a, b)| {
            let a = usize::from(a);
            let b = usize::from(b);
            idx >= a && idx < b
        });
        if !in_range {
            out.push(line.clone());
        } else if wrap_enabled {
            out.extend(char_wrap_tint(line, bg, width));
        } else {
            out.push(tint_line(line, bg, width));
        }
    }
    out
}
