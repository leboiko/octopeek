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
        Focus::Detail => "DETAIL",
        Focus::RepoPicker => "REPOS",
        Focus::Help => "HELP",
    };

    // Left: fetch indicator or nothing.
    let fetch_indicator = if app.fetching {
        Span::styled(" syncing... ", Style::default().fg(p.dim))
    } else {
        Span::raw(" ")
    };

    // Center: flash message (if active) or empty.
    let center_text =
        flash.filter(|m| m.is_active()).map_or_else(String::new, |m| format!("  {}  ", m.text));

    // Right: compact keybinding hints for current focus.
    let hints = match app.focus {
        Focus::Dashboard => "j/k nav  Enter detail  i toggle  r refresh  ? help  q quit",
        Focus::Detail => {
            "j/k scroll  Tab section  n/N unresolved  o browser  y copy  Esc back  q quit"
        }
        Focus::RepoPicker => "j/k nav  Enter select  Esc close  ? help  q quit",
        Focus::Help => "? / Esc / q close help",
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" {focus_label} "),
            Style::default().fg(p.on_accent_fg).bg(p.accent).add_modifier(Modifier::BOLD),
        ),
        fetch_indicator,
        Span::styled(center_text, Style::default().fg(p.status_bar_fg)),
        Span::styled(format!(" {hints} "), Style::default().fg(p.dim)),
    ]);

    let paragraph = Paragraph::new(line).style(Style::default().bg(p.status_bar_bg));
    f.render_widget(paragraph, area);
}
