//! Theme picker overlay for live theme selection.
//!
//! Renders a centered modal displaying all available [`Theme`] variants.
//! While open, the palette updates immediately on cursor movement so the
//! user sees a live preview. Pressing `Enter` persists the choice; `Esc`
//! reverts to the original theme without saving.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::App;
use crate::theme::Theme;

// Modal dimensions.
const MODAL_WIDTH: u16 = 46;
// 2 padding rows + N theme rows + 1 blank + 1 footer = N + 4
// Theme::ALL.len() == 8 → 12 rows + 2 border = 14. Add a blank top row → 15.
const MODAL_HEIGHT: u16 = 15;

/// Render the theme picker overlay centered in the terminal.
///
/// Must be drawn **after** all other widgets so it floats on top.
///
/// # Arguments
///
/// * `f`   - Ratatui frame to render into.
/// * `app` - Shared application state; reads `palette`, `theme_picker_cursor`,
///   and `config.theme`.
pub fn draw(f: &mut Frame, app: &App) {
    let area = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, f.area());
    f.render_widget(Clear, area);

    let p = &app.palette;

    let block = Block::default()
        .title(" Theme ")
        .title_style(Style::default().fg(p.accent).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.border_focused))
        .style(Style::default().bg(p.help_bg));

    let lines = build_lines(app);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Build the content lines for the theme picker modal.
fn build_lines(app: &App) -> Vec<Line<'static>> {
    let p = &app.palette;
    let cursor = app.theme_picker_cursor;
    let active_theme = app.config.theme;

    let cursor_style =
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD);
    let active_style =
        Style::default().fg(p.accent_alt).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(p.foreground);
    let dim_style = p.dim_style();

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(Theme::ALL.len() + 4);
    lines.push(Line::from(""));

    for (idx, &theme) in Theme::ALL.iter().enumerate() {
        let is_cursor = idx == cursor;
        let is_active = theme == active_theme;
        lines.push(theme_row(is_cursor, is_active, theme.label(), cursor_style, active_style, text_style, dim_style));
    }

    lines.push(Line::from(""));

    // Footer hint line.
    lines.push(Line::from(vec![
        Span::styled("  ", text_style),
        Span::styled("\u{2191}\u{2193} move", cursor_style),
        Span::styled("  ", dim_style),
        Span::styled("Enter", cursor_style),
        Span::styled(" apply  ", dim_style),
        Span::styled("Esc", cursor_style),
        Span::styled(" cancel", dim_style),
    ]));

    lines
}

/// Render a single theme row with arrow/bullet indicators.
fn theme_row(
    is_cursor: bool,
    is_active: bool,
    label: &'static str,
    cursor_style: Style,
    active_style: Style,
    text_style: Style,
    dim_style: Style,
) -> Line<'static> {
    let arrow = if is_cursor { "> " } else { "  " };
    let bullet = if is_active { "\u{25cf}" } else { "\u{25cb}" };
    let bullet_style = if is_active { active_style } else { dim_style };
    let label_style = if is_cursor { cursor_style } else { text_style };

    Line::from(vec![
        Span::styled("  ", text_style),
        Span::styled(arrow, cursor_style),
        Span::styled(bullet, bullet_style),
        Span::styled(" ", text_style),
        Span::styled(label, label_style),
    ])
}

/// Compute a centered [`Rect`] of the requested dimensions within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical =
        Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).split(area);
    Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center).split(vertical[0])[0]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn make_app_at_theme(theme: Theme) -> crate::app::App {
        let config = crate::config::Config { theme, ..Default::default() };
        let session = crate::state::AppSession::default();
        let mut app = crate::app::App::new(config, session);
        // Position cursor on the active theme so the highlight is visible.
        app.theme_picker_cursor =
            Theme::ALL.iter().position(|&t| t == theme).unwrap_or(0);
        app.focus = crate::app::Focus::ThemePicker;
        app
    }

    /// The overlay must render every theme label in the buffer.
    #[test]
    fn draw_shows_all_theme_labels() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let app = make_app_at_theme(Theme::Default);

        terminal.draw(|f| draw(f, &app)).expect("draw");

        let buffer = terminal.backend().buffer();
        let rendered: String =
            buffer.content.iter().map(ratatui::buffer::Cell::symbol).collect();

        for theme in Theme::ALL {
            assert!(
                rendered.contains(theme.label()),
                "label '{}' must appear in buffer",
                theme.label()
            );
        }
    }

    /// The title " Theme " must appear in the overlay.
    #[test]
    fn draw_shows_title() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let app = make_app_at_theme(Theme::Nord);

        terminal.draw(|f| draw(f, &app)).expect("draw");

        let buffer = terminal.backend().buffer();
        let rendered: String =
            buffer.content.iter().map(ratatui::buffer::Cell::symbol).collect();
        assert!(rendered.contains("Theme"), "modal title must be rendered");
    }
}
