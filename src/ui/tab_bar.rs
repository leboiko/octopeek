//! Tab bar widget rendered above the main content area.
//!
//! Each tab shows the repo name (`owner/name`). In Phase 3, a badge with the
//! count of items needing attention will be appended.

use crate::app::App;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr as _;

/// Maximum display width (cells) for a tab label before truncation.
const MAX_LABEL_WIDTH: usize = 24;

/// Display width (cells) of padding on each side of a label.
const PAD_EACH_SIDE: usize = 1;

/// Truncate `label` to at most `MAX_LABEL_WIDTH` display cells.
///
/// Truncation preserves the repo owner prefix and uses `…` as the ellipsis so
/// the label stays recognisable (e.g. `rust-lang/ve…` instead of `rust-lang/v`).
fn truncate_label(label: &str) -> String {
    use unicode_width::UnicodeWidthChar as _;

    let display_width = label.width();
    if display_width <= MAX_LABEL_WIDTH {
        return label.to_owned();
    }

    // Collect characters until we hit MAX_LABEL_WIDTH - 1 (leaving room for
    // the ellipsis character).
    let limit = MAX_LABEL_WIDTH - 1;
    let mut out = String::new();
    let mut used = 0usize;
    for ch in label.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > limit {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// Compute the display-cell width of a padded tab label.
fn label_cell_width(label: &str) -> u16 {
    // " {label} " — one space on each side.
    crate::cast::u16_sat(label.width() + PAD_EACH_SIDE * 2)
}

/// Compute the `[start, end)` index range of tabs that fits within
/// `available_width`, guaranteeing the active tab is always included.
///
/// The algorithm greedily expands left then right from `active_idx`, reserving
/// `overflow_reserve` cells for any `+N` indicator that must be shown on the
/// side(s) where tabs are hidden.
fn visible_window(
    widths: &[u16],
    active_idx: usize,
    available_width: u16,
    overflow_reserve: u16,
) -> (usize, usize) {
    let n = widths.len();
    if n == 0 {
        return (0, 0);
    }

    let mut start = active_idx;
    let mut end = active_idx + 1;
    let mut used: u16 = widths[active_idx];

    loop {
        let mut expanded = false;

        if start > 0 {
            let extra = widths[start - 1];
            let reserve_l = if start > 1 { overflow_reserve } else { 0 };
            let reserve_r = if end < n { overflow_reserve } else { 0 };
            if used.saturating_add(extra).saturating_add(reserve_l).saturating_add(reserve_r)
                <= available_width
            {
                start -= 1;
                used = used.saturating_add(extra);
                expanded = true;
            }
        }

        if end < n {
            let extra = widths[end];
            let reserve_l = if start > 0 { overflow_reserve } else { 0 };
            let reserve_r = if end + 1 < n { overflow_reserve } else { 0 };
            if used.saturating_add(extra).saturating_add(reserve_l).saturating_add(reserve_r)
                <= available_width
            {
                end += 1;
                used = used.saturating_add(extra);
                expanded = true;
            }
        }

        if !expanded {
            break;
        }
    }

    (start, end)
}

/// Render the tab bar into `area`.
///
/// Renders nothing when no tabs are open.
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    // `+K` overflow indicator occupies at most 5 cells " +32 ".
    const OVERFLOW_MAX: u16 = 5;

    if app.tabs.is_empty() {
        return;
    }

    let p = &app.palette;
    let n = app.tabs.len();
    let active_idx = app.tabs.active_index().unwrap_or(0);

    // Build display label strings.
    // Format: ` N: owner/name ` where N is the 1-based index (hidden past 9).
    let labels: Vec<String> = app
        .tabs
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let num = if i < 9 { format!("{}", i + 1) } else { " ".to_string() };
            let name = truncate_label(&tab.repo);
            // Phase 3: append needs_action_count badge here when available.
            format!(" {num}: {name} ")
        })
        .collect();

    let widths: Vec<u16> = labels.iter().map(|l| label_cell_width(l)).collect();
    let (start, end) = visible_window(&widths, active_idx, area.width, OVERFLOW_MAX);

    let hidden_before = start;
    let hidden_after = n.saturating_sub(end);

    let mut spans: Vec<Span> = Vec::new();

    for (i, label) in labels.iter().enumerate().skip(start).take(end - start) {
        let style = if i == active_idx {
            Style::default().fg(p.on_accent_fg).bg(p.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.dim).bg(p.status_bar_bg)
        };
        spans.push(Span::styled(label.clone(), style));
    }

    if hidden_before > 0 {
        spans.insert(
            0,
            Span::styled(
                format!(" +{hidden_before} "),
                Style::default().fg(p.accent_alt).bg(p.status_bar_bg),
            ),
        );
    }
    if hidden_after > 0 {
        spans.push(Span::styled(
            format!(" +{hidden_after} "),
            Style::default().fg(p.accent_alt).bg(p.status_bar_bg),
        ));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).style(Style::default().bg(p.status_bar_bg));
    f.render_widget(paragraph, area);
}
