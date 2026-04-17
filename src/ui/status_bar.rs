//! Single-line status bar rendered at the bottom of the screen.
// `FlashMessage::new` is used by action handlers in Phase 3+.
#![allow(dead_code)]
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

    let hint_text = if let Some(msg) = flash.filter(|m| m.is_active()) {
        // A flash message is active — show it instead of the default hints.
        format!("  {}", msg.text)
    } else {
        " Tab:next-tab  ?:help  r:refresh  i:toggle-view  o:open  y:copy-url  c:checkout  q:quit"
            .to_owned()
    };

    let active_repo =
        app.tabs.active_tab().map_or_else(|| "no repos configured".to_owned(), |t| t.repo.clone());

    let line = Line::from(vec![
        Span::styled(
            format!(" {focus_label} "),
            Style::default().fg(p.on_accent_fg).bg(p.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {active_repo}"), Style::default().fg(p.status_bar_fg)),
        Span::styled(hint_text, Style::default().fg(p.dim)),
    ]);

    let paragraph = Paragraph::new(line).style(Style::default().bg(p.status_bar_bg));
    f.render_widget(paragraph, area);
}
