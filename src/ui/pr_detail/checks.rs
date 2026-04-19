//! Check-run helpers and line builder for the Checks section.

use ratatui::{
    style::{Style},
    text::{Line, Span},
};

use crate::github::detail::{DetailedCheck, PrDetail};
use crate::theme::Palette;

/// `true` when a check conclusion indicates failure that the viewer must fix.
pub(super) fn check_is_failing(check: &DetailedCheck) -> bool {
    matches!(
        check.conclusion.as_deref(),
        Some("FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED")
    )
}

/// Glyph for a single check run: `✔`, `✖`, `●`, or `—`.
pub(super) fn check_glyph(check: &DetailedCheck) -> &'static str {
    match check.conclusion.as_deref() {
        Some("SUCCESS") => "\u{2714}", // ✔
        Some("FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED") => {
            "\u{2716}" // ✖
        }
        None if check.status != "COMPLETED" => "\u{25CF}", // ● in-progress
        _ => "\u{2014}",                                   // — no conclusion on completed
    }
}

/// Format `duration_seconds` as `Xm Ys`.
pub(super) fn fmt_duration(secs: u64) -> String {
    if secs < 60 { format!("{secs}s") } else { format!("{}m {}s", secs / 60, secs % 60) }
}

/// Build check-run lines (up to 8, with overflow footer).
pub(super) fn checks_lines(detail: &PrDetail, p: &Palette) -> Vec<Line<'static>> {
    let mut checks: Vec<&DetailedCheck> = detail.check_runs.iter().collect();
    // Failing checks sorted first.
    checks.sort_by_key(|c| !check_is_failing(c));

    let visible = checks.len().min(8);
    let overflow = checks.len().saturating_sub(8);

    let mut lines = Vec::with_capacity(visible + 1);
    for check in &checks[..visible] {
        let glyph = check_glyph(check);
        let glyph_color = if check_is_failing(check) {
            p.danger
        } else if check.conclusion.as_deref() == Some("SUCCESS") {
            p.success
        } else {
            p.muted
        };

        let workflow_prefix =
            check.workflow_name.as_deref().map(|wf| format!("{wf} / ")).unwrap_or_default();

        let duration_str =
            check.duration_seconds.map(|s| format!(" ({})", fmt_duration(s))).unwrap_or_default();

        let status_text = check.conclusion.as_deref().unwrap_or(&check.status).to_lowercase();

        lines.push(Line::from(vec![
            Span::styled(format!("{glyph} "), Style::default().fg(glyph_color)),
            Span::styled(workflow_prefix, Style::default().fg(p.dim)),
            Span::styled(check.name.clone(), Style::default().fg(p.foreground)),
            Span::styled(format!(" [{status_text}]"), Style::default().fg(p.muted)),
            Span::styled(duration_str, Style::default().fg(p.dim)),
        ]));
    }

    if overflow > 0 {
        lines.push(Line::from(Span::styled(
            format!("  ... {overflow} more"),
            Style::default().fg(p.dim),
        )));
    }

    lines
}
