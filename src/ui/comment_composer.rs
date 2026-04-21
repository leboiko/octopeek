//! Markdown composer overlay for PR/issue comments and review-thread replies.

use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

/// Render the composer overlay.
pub fn draw(f: &mut Frame, app: &App) {
    let Some(composer) = &app.composer else {
        return;
    };

    let p = &app.palette;
    let area = centered_rect(78, 16, f.area());
    let title = format!(" {} ", composer.target.label());
    let block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.border_focused))
        .style(Style::default().bg(p.help_bg));
    let inner = block.inner(area);

    let body = if composer.body.is_empty() {
        vec![Line::from(Span::styled(
            "Write markdown...",
            Style::default().fg(p.dim).add_modifier(Modifier::DIM),
        ))]
    } else {
        composer.body.lines().map(|line| Line::from(line.to_owned())).collect()
    };

    let hint = Line::from(vec![
        Span::styled("Ctrl+S", Style::default().fg(p.success).add_modifier(Modifier::BOLD)),
        Span::styled(" submit   ", Style::default().fg(p.dim)),
        Span::styled("Enter", Style::default().fg(p.accent).add_modifier(Modifier::BOLD)),
        Span::styled(" newline   ", Style::default().fg(p.dim)),
        Span::styled("Esc", Style::default().fg(p.danger).add_modifier(Modifier::BOLD)),
        Span::styled(" cancel", Style::default().fg(p.dim)),
    ]);

    let split = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(inner);

    f.render_widget(Clear, area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(body)
            .style(Style::default().fg(p.foreground).bg(p.help_bg))
            .wrap(Wrap { trim: false }),
        split[0],
    );
    f.render_widget(Paragraph::new(hint).style(Style::default().bg(p.help_bg)), split[1]);
}

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
