//! UI rendering: one `draw` function composes all panels for a single frame.

pub mod dashboard;
pub mod help;
pub mod pr_detail;
pub mod repo_picker;
pub mod status_bar;
pub mod tab_bar;
pub mod tabs;

use crate::app::App;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Render the full application UI for one frame.
///
/// Layout (top to bottom):
/// 1. Tab bar (1 row, only when tabs are open)
/// 2. Main content area (fills remaining height)
/// 3. Status bar (1 row)
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();

    // Paint every cell with the theme background before any widget renders.
    // Without this, cells not covered by a widget retain the terminal's default
    // background, making light themes look broken on dark terminals.
    f.render_widget(Block::default().style(Style::default().bg(app.palette.background)), area);

    let has_tabs = !app.tabs.is_empty();
    let tab_bar_height: u16 = u16::from(has_tabs);

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(tab_bar_height),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    // ── Tab bar ───────────────────────────────────────────────────────────────
    if has_tabs {
        tab_bar::draw(f, app, outer[0]);
    }

    // ── Main content area ─────────────────────────────────────────────────────
    let content_area = outer[1];

    let placeholder_block = Block::default()
        .borders(Borders::ALL)
        .border_style(app.palette.border_style())
        .style(Style::default().bg(app.palette.background));

    let placeholder_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "octopeek",
            Style::default().fg(app.palette.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled("v0.1.0", Style::default().fg(app.palette.dim))),
        Line::from(""),
        Line::from(Span::styled(
            "Phase 1 scaffold — data layer coming in Phase 2",
            Style::default().fg(app.palette.foreground),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Add repos to ~/.config/octopeek/config.toml to get started.",
            Style::default().fg(app.palette.dim),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press ? for keybinding help, q to quit.",
            Style::default().fg(app.palette.dim),
        )),
    ];

    let placeholder =
        Paragraph::new(placeholder_text).block(placeholder_block).alignment(Alignment::Center);

    f.render_widget(placeholder, content_area);

    // ── Status bar ────────────────────────────────────────────────────────────
    // Phase 2+: wire a real FlashMessage through App state.
    status_bar::draw(f, app, None, outer[2]);

    // ── Help overlay (drawn last so it floats above everything) ───────────────
    if app.show_help {
        help::draw(f, app);
    }
}
