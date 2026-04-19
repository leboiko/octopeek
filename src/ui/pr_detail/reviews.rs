//! Review line builder for the Reviews section.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::github::detail::PrDetail;
use crate::github::types::ReviewState;
use crate::theme::Palette;
use crate::ui::util::humanize_delta;
use crate::ui::util::truncate;

/// Build review lines (one or two lines per review).
pub(super) fn reviews_lines(detail: &PrDetail, p: &Palette) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for review in &detail.reviews {
        let (verdict, color) = match review.state {
            ReviewState::Approved => ("approved", p.success),
            ReviewState::ChangesRequested => ("changes requested", p.danger),
            ReviewState::Commented => ("commented", p.muted),
            ReviewState::Dismissed => ("dismissed (dismissed)", p.muted),
            ReviewState::Pending => ("pending", p.dim),
        };

        let age = humanize_delta(&review.submitted_at);
        lines.push(Line::from(vec![
            Span::styled("\u{25CF} ", Style::default().fg(color)), // ●
            Span::styled(format!("@{}", review.author), Style::default().fg(p.foreground)),
            Span::styled(format!(" {verdict}"), Style::default().fg(color)),
            Span::styled(format!(" {age}"), Style::default().fg(p.dim)),
        ]));

        // If the review has a body, show first 80 chars truncated in dim.
        let body = review.body_markdown.trim();
        if !body.is_empty() {
            let first_line = body.lines().next().unwrap_or(body);
            lines.push(Line::from(Span::styled(
                format!("    {}", truncate(first_line, 80)),
                Style::default().fg(p.dim),
            )));
        }
    }

    // Suppress unused import warning: `Modifier` is referenced below in case
    // reviews ever get bold treatment, and is imported for consistency with
    // other section renderers.
    let _ = Modifier::BOLD;

    lines
}
