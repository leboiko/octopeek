//! Commits section builder: one row per commit, newest-first.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::github::detail::PrDetail;
use crate::theme::Palette;
use crate::ui::util::{humanize_delta, section_header, truncate};

// ── Column widths ─────────────────────────────────────────────────────────────
//
// Layout for an 80-column terminal (inner width after padding):
//
//   7  SHA + 1 space
//   varies  headline (flex)
//   1 space
//   8  "@author" (prefix included) — truncated to 12 chars total
//   1 space
//   8  relative age ("123d ago" max)
//   1 space
//   4  "+N" additions
//   1 space
//   4  "-M" deletions
//
// At 60 cols we drop age first, then stats, to keep SHA + headline readable.

/// Fixed width reserved for the short SHA column (7 chars + 1 trailing space).
const SHA_COLS: usize = 8;

/// Fixed width for the `@author` column (prefix `@` + up to 11 chars = 12, + 1 space).
const AUTHOR_COLS: usize = 13;

/// Fixed width for the relative-age column (e.g. "123d ago" = 8 chars + 1 space).
const AGE_COLS: usize = 9;

/// Fixed width for the additions column (e.g. "+1234" = 5 chars + 1 space).
const ADDS_COLS: usize = 6;

/// Fixed width for the deletions column (e.g. "-1234" = 5 chars).
const DELS_COLS: usize = 5;

/// Minimum terminal width below which we start dropping optional columns.
const DROP_STATS_BELOW: usize = 60;

/// Minimum terminal width below which we drop the age column as well.
const DROP_AGE_BELOW: usize = 50;

/// Build lines for the Commits section.
///
/// Returns `(lines, alt_bg_ranges)` — ranges are always empty for this section
/// (no stripe tinting needed for a compact list view).
///
/// # Arguments
///
/// * `detail` - The loaded PR detail (commits already sorted newest-first).
/// * `p`      - Active colour palette.
pub(super) fn build_commits(
    detail: &PrDetail,
    p: &Palette,
) -> (Vec<Line<'static>>, Vec<(u16, u16)>) {
    if detail.commits.is_empty() {
        return (Vec::new(), Vec::new());
    }

    // Use a generous proxy for the available width. The right-pane inner width
    // isn't threaded into this function (it requires a Frame/Rect); 80 columns
    // is a safe conservative default that matches the column-budget above. The
    // Paragraph widget clips long lines rather than wrapping, so over-estimating
    // is harmless. Under-estimating would truncate headlines unnecessarily.
    let avail: usize = 80;

    let show_stats = avail >= DROP_STATS_BELOW;
    let show_age = avail >= DROP_AGE_BELOW;

    // Compute the budget for the headline column.
    let mut fixed = SHA_COLS + AUTHOR_COLS;
    if show_age {
        fixed += AGE_COLS;
    }
    if show_stats {
        fixed += ADDS_COLS + DELS_COLS;
    }
    // Leave at least 10 chars for the headline before truncation kicks in.
    let headline_cols = avail.saturating_sub(fixed).max(10);

    let count = detail.commits.len();
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(count + 2);

    // Section header: "COMMITS (N)" in accent bold, matching the Comments /
    // Reviews section-header pattern via the shared `section_header` helper.
    lines.push(section_header(&format!("COMMITS ({count})"), p));
    lines.push(Line::from(""));

    for commit in &detail.commits {
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(8);

        // Column 1: short SHA (7 chars) in muted colour.
        spans.push(Span::styled(format!("{:<7} ", commit.short_sha), Style::default().fg(p.muted)));

        // Column 2: message headline — flex, truncated to budget.
        let headline = truncate(&commit.headline, headline_cols);
        // Pad to the full budget so the next column aligns consistently.
        let headline_padded = format!("{headline:<headline_cols$} ");
        spans.push(Span::styled(headline_padded, Style::default().fg(p.foreground)));

        // Column 3: `@author` — truncated to 11 chars after `@` prefix.
        let author_trunc = truncate(&commit.author, 11);
        spans.push(Span::styled(format!("@{author_trunc:<11} "), Style::default().fg(p.dim)));

        // Column 4 (optional): relative age.
        if show_age {
            let age = humanize_delta(&commit.committed_at);
            spans.push(Span::styled(format!("{age:<8} "), Style::default().fg(p.dim)));
        }

        // Columns 5-6 (optional): `+N -M` diff stats.
        if show_stats {
            spans.push(Span::styled(
                format!("+{:<5}", commit.additions),
                Style::default().fg(p.git_new),
            ));
            spans.push(Span::styled(
                format!("-{:<5}", commit.deletions),
                Style::default().fg(p.danger),
            ));
        }

        lines.push(Line::from(spans));
    }

    (lines, Vec::new())
}
