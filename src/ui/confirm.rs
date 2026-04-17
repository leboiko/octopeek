//! Confirmation overlay widget.
//!
//! Renders a centered modal that asks the user to confirm or cancel an action.
//! The overlay is generic over [`ConfirmPending`], which carries the details of
//! the action to execute if the user presses `y`.
//!
//! # Extensibility
//!
//! New confirmation flows (e.g. "Confirm merge PR", "Confirm close issue") are
//! added by:
//!
//! 1. Appending a variant to [`ConfirmPending`].
//! 2. Adding a match arm in `App::execute_confirm` (in `app/mod.rs`).
//!
//! The overlay rendering code itself does not need to change — it is driven
//! entirely by the `title` and `prompt` strings stored in [`Confirm`].

use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

// ── ConfirmPending ─────────────────────────────────────────────────────────────

/// The action to execute when the user confirms.
///
/// Each variant encodes all the data needed to perform the action without
/// re-reading `App` state — this makes the execution path simple and testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmPending {
    /// Check out a PR's head branch in the current working directory.
    CheckoutBranch {
        /// `owner/name` repository slug.
        repo: String,
        /// PR number (used for flash message context only).
        number: u32,
        /// Branch name to pass to `git checkout`.
        branch: String,
    },
}

// ── Confirm ────────────────────────────────────────────────────────────────────

/// State for the confirmation overlay.
///
/// Store this in `App::confirm: Option<Confirm>`.  When `Some`, the overlay is
/// rendered on top of the current focus panel.  `Focus::Confirm` must be set
/// simultaneously so key events are routed here.
#[derive(Debug, Clone)]
pub struct Confirm {
    /// Short title shown in the overlay border (e.g. `"Checkout branch"`).
    pub title: String,
    /// Human-readable question shown in the overlay body.
    pub prompt: String,
    /// The action that will be executed when the user presses `y`.
    pub pending_action: ConfirmPending,
}

// ── Rendering ─────────────────────────────────────────────────────────────────

/// Render the confirmation overlay centered in the terminal.
///
/// The caller is responsible for drawing this **after** all other widgets so
/// the overlay floats on top.
pub fn draw(f: &mut Frame, app: &App) {
    let Some(confirm) = &app.confirm else {
        return;
    };

    let p = &app.palette;
    let area = centered_rect(64, 9, f.area());

    let title = format!(" {} ", confirm.title);
    let block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.border_focused))
        .style(Style::default().bg(p.help_bg));

    let hint_yes =
        Span::styled("[y] yes", Style::default().fg(p.success).add_modifier(Modifier::BOLD));
    let hint_no =
        Span::styled("[N] no / cancel", Style::default().fg(p.dim).add_modifier(Modifier::BOLD));

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(confirm.prompt.as_str(), Style::default().fg(p.foreground))),
        Line::from(""),
        Line::from(vec![Span::raw("  "), hint_yes, Span::raw("   "), hint_no]),
    ];

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });

    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}

// ── Layout helper ─────────────────────────────────────────────────────────────

/// Return a centered `Rect` of the given `width` and `height` within `area`.
///
/// Clamps to the terminal size if the requested dimensions exceed it.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);

    let [_, center_v, _] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(h), Constraint::Fill(1)])
            .flex(Flex::Center)
            .areas(area);

    let [_, center_h, _] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(w), Constraint::Fill(1)])
            .flex(Flex::Center)
            .areas(center_v);

    center_h
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// `ConfirmPending` must be clonable and comparable (used in state transition tests).
    #[test]
    fn confirm_pending_clone_eq() {
        let pending = ConfirmPending::CheckoutBranch {
            repo: "owner/repo".to_owned(),
            number: 42,
            branch: "feat/my-feature".to_owned(),
        };
        assert_eq!(pending.clone(), pending);
    }

    /// `Confirm` must carry all fields through construction.
    #[test]
    fn confirm_fields_accessible() {
        let c = Confirm {
            title: "Checkout branch".to_owned(),
            prompt: "Checkout `feat/foo` in /home/user/project?".to_owned(),
            pending_action: ConfirmPending::CheckoutBranch {
                repo: "owner/repo".to_owned(),
                number: 1,
                branch: "feat/foo".to_owned(),
            },
        };
        assert_eq!(c.title, "Checkout branch");
        assert!(c.prompt.contains("feat/foo"));
        matches!(c.pending_action, ConfirmPending::CheckoutBranch { .. });
    }
}
