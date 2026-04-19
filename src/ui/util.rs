//! Shared UI utilities used across multiple rendering modules.

use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph},
};

use crate::theme::Palette;

/// Format `dt` as a human-readable delta from now: "14s ago", "3m ago", "1h ago", etc.
///
/// # Arguments
///
/// * `dt` - The timestamp to humanize relative to `Utc::now()`.
///
/// # Returns
///
/// A short human-readable string representing the elapsed time.
pub fn humanize_delta(dt: &DateTime<Utc>) -> String {
    // `.max(0)` ensures non-negative before casting; `cast_unsigned` is not
    // available on stable Rust 1.88. The `.max(0)` guard makes the sign loss safe.
    #[allow(clippy::cast_sign_loss)]
    let secs = (Utc::now() - *dt).num_seconds().max(0) as u64;
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Truncate `s` to at most `max_chars` Unicode characters, appending `…` if truncated.
///
/// # Arguments
///
/// * `s`         - The string to truncate.
/// * `max_chars` - Maximum number of Unicode scalar values in the result.
pub fn truncate(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('\u{2026}'); // …
    out
}

/// Build a bold section-header line with a short leading rule.
///
/// The three-char heavy rule (`━━━`) plus the label gives each section a clear
/// visual break. Keeping it on one line prevents long labels from wrapping
/// mid-rule.
///
/// # Arguments
///
/// * `label` - The section label text (e.g. `"COMMENTS (3)"`).
/// * `p`     - Active colour palette.
///
/// # Returns
///
/// A single [`Line`] with accent-coloured rule and bold label.
pub(crate) fn section_header(label: &str, p: &Palette) -> Line<'static> {
    let rule = "\u{2501}".repeat(3); // ━━━
    Line::from(vec![
        Span::styled(format!("{rule} "), Style::default().fg(p.accent)),
        Span::styled(label.to_owned(), Style::default().fg(p.accent).add_modifier(Modifier::BOLD)),
    ])
}

/// Render a sticky detail header: tinted block above a full-width accent rule.
///
/// Both `pr_detail` and `issue_detail` share this exact layout. The tinted
/// block (`help_bg`) matches the comment-stripe tone so the header reads as
/// the same "card" surface. A heavy bottom rule (`━`) in `accent` separates
/// the header from the scrolling body.
///
/// # Arguments
///
/// * `f`     - Ratatui frame to render into.
/// * `lines` - Content lines for the header block.
/// * `area`  - Target rectangle (includes the rule row).
/// * `p`     - Active colour palette.
pub(crate) fn render_detail_header(
    f: &mut Frame,
    lines: Vec<Line<'static>>,
    area: Rect,
    p: &Palette,
) {
    if area.height == 0 {
        return;
    }
    // Split: content rows + one bottom rule row.
    let rule_row = area.height.saturating_sub(1);
    let content_h = area.height.saturating_sub(1);

    let content_area = Rect { x: area.x, y: area.y, width: area.width, height: content_h };
    let rule_area = Rect { x: area.x, y: area.y + rule_row, width: area.width, height: 1 };

    let block = Block::default()
        .style(Style::default().bg(p.help_bg).fg(p.foreground))
        .padding(Padding::new(2, 2, 1, 0));
    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, content_area);

    // Full-width heavy rule so the separator reads as a deliberate section
    // break rather than a faint line artefact.
    let rule_text = "\u{2501}".repeat(usize::from(rule_area.width));
    let rule = Paragraph::new(Line::from(Span::styled(
        rule_text,
        Style::default().fg(p.accent).bg(p.background),
    )));
    f.render_widget(rule, rule_area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_unchanged() {
        assert_eq!(truncate("hello", 20), "hello");
    }

    #[test]
    fn truncate_at_limit_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_appends_ellipsis() {
        let t = truncate("hello world", 8);
        assert!(t.chars().count() <= 8);
        assert!(t.ends_with('\u{2026}'));
    }

    #[test]
    fn truncate_zero_max_returns_empty() {
        assert_eq!(truncate("anything", 0), "");
    }
}
