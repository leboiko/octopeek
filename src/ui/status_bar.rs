//! Single-line status bar rendered at the bottom of the screen.
//!
//! Shows the current focus mode, keybinding hints, and any active flash
//! message. Flash messages auto-revert after a configurable duration (the
//! timer is polled on each draw call).

use std::time::Instant;

use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// A short message to display in the status bar for a limited time.
#[derive(Debug, Clone)]
pub struct FlashMessage {
    pub text: String,
    /// When the message should disappear.
    pub expires_at: Instant,
}

impl FlashMessage {
    /// Create a flash message that expires after `duration`.
    #[allow(dead_code)] // Used by Phase 3+ action handlers (copy URL, checkout).
    pub fn new(text: impl Into<String>, duration: std::time::Duration) -> Self {
        Self { text: text.into(), expires_at: Instant::now() + duration }
    }

    /// Return `true` if the message has not yet expired.
    pub fn is_active(&self) -> bool {
        Instant::now() < self.expires_at
    }
}

/// Render the single-line status bar into `area`.
///
/// `flash` is checked each draw call; expired messages are not shown.
pub fn draw(f: &mut Frame, app: &App, flash: Option<&FlashMessage>, area: Rect) {
    let p = &app.palette;

    let focus_label = match app.focus {
        Focus::Dashboard => "DASHBOARD",
        Focus::FirstRun => "WELCOME",
        // Copy mode is a modal sublayer of the Detail focus; surface it in the
        // badge so the changed keymap (h/j/k/l cursor, V select, y yank) is
        // obvious at a glance.
        Focus::Detail if app.copy_mode.active => "COPY",
        Focus::Detail => "DETAIL",
        Focus::RepoPicker => "REPOS",
        Focus::Help => "HELP",
        Focus::Confirm => "CONFIRM",
        Focus::ThemePicker => "THEME",
    };

    // Left: fetch indicator — inbox sync takes priority, then detail SWR.
    let fetch_indicator = if app.fetching {
        Span::styled(" syncing... ", Style::default().fg(p.dim))
    } else if app.detail_refreshing.is_some() {
        Span::styled(" refreshing... ", Style::default().fg(p.dim))
    } else if let Some((ready, total, in_flight)) = app.commit_diff_cache_counts()
        && ready < total
        && in_flight > 0
    {
        Span::styled(format!(" warming diffs {ready}/{total}... "), Style::default().fg(p.warning))
    } else {
        Span::raw(" ")
    };

    // Center: flash message (if active) or empty.
    let center_text =
        flash.filter(|m| m.is_active()).map_or_else(String::new, |m| format!("  {}  ", m.text));

    // Right: compact keybinding hints for current focus.
    let hints = match app.focus {
        Focus::Dashboard => {
            "j/k nav  Enter detail  i toggle  A all/mine  r refresh  c theme  p repos  ? help  q quit"
        }
        Focus::FirstRun => "Space toggle  Enter confirm  a add  Esc skip",
        Focus::Detail if app.copy_mode.active => {
            "h/j/k/l move  0/$ line ends  V select  y yank  Y yank line  Esc exit"
        }
        Focus::Detail => {
            "!@#$% sections  C commits  Enter commit diff  Esc commits/back  J/K file  j/k scroll  v copy"
        }
        Focus::RepoPicker => "j/k nav  a add  d delete  Enter select  Esc close",
        Focus::Help => "? / Esc / q close help",
        Focus::Confirm => "[y] confirm  [N] / Esc cancel",
        Focus::ThemePicker => "j/k move  Enter apply  Esc cancel",
    };

    // Commit-scope badge: appended after the hints when the user has scoped
    // the Files section to a single commit's delta.
    let commit_scope_span: Option<Span<'static>> =
        if let (Some(idx), Focus::Detail) = (app.selected_commit, app.focus) {
            app.pr_detail.as_ref().and_then(|d| d.commits.get(idx)).map(|c| {
                let glyph = if app.config.show_ascii_glyphs { "@" } else { "\u{25c8}" }; // ◈
                Span::styled(
                    format!("  {glyph} {}   H\u{2192}HEAD ", c.short_sha),
                    Style::default().fg(p.warning),
                )
            })
        } else {
            None
        };

    let mut spans = vec![
        Span::styled(
            format!(" {focus_label} "),
            Style::default().fg(p.on_accent_fg).bg(p.accent).add_modifier(Modifier::BOLD),
        ),
        fetch_indicator,
        Span::styled(center_text, Style::default().fg(p.status_bar_fg)),
        Span::styled(format!(" {hints} "), Style::default().fg(p.dim)),
    ];
    if let Some(scope_span) = commit_scope_span {
        spans.push(scope_span);
    }
    let line = Line::from(spans);

    let paragraph = Paragraph::new(line).style(Style::default().bg(p.status_bar_bg));
    f.render_widget(paragraph, area);
}
