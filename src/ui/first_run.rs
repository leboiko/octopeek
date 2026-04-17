//! First-run welcome wizard overlay.
//!
//! Displayed the first time `octopeek` is launched with an empty config when
//! the GitHub inbox fetch discovers repositories the user is already active in.
//! The wizard lets the user multi-select those repos for tracking before the
//! dashboard is shown.
//!
//! # Layout
//!
//! A centered panel (~70 cols wide, ~24 rows tall, clamped to the terminal)
//! floats above the dashboard using a `Clear` + bordered `Block` pattern
//! identical to the help and repo-picker overlays.
//!
//! # Edge case
//!
//! When `app.first_run_suggestions` is unexpectedly empty (the inbox loaded
//! but the user has no open items in any repo) a different centered message
//! is shown instructing the user to add a repo manually or press `Esc`.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::App;

// ── Public entry point ────────────────────────────────────────────────────────

/// Render the first-run wizard overlay centered in the terminal.
///
/// Must be called **after** all base widgets (dashboard, tab bar, status bar)
/// so the overlay floats on top.
///
/// # Arguments
///
/// * `f` - Active ratatui frame.
/// * `app` - Immutable reference to the full application state; reads
///   `first_run_suggestions`, `first_run_cursor`, and the active `palette`.
pub fn draw(f: &mut Frame, app: &App) {
    let p = &app.palette;
    let area = wizard_rect(f.area());

    // Paint the overlay background via Clear + bordered block (same pattern as
    // `help::draw` and `repo_picker::draw`).
    let block = Block::default()
        .title(" Welcome to octopeek ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.border_focused))
        .style(Style::default().bg(p.help_bg));

    f.render_widget(Clear, area);
    f.render_widget(block, area);

    // 1-cell padding inside the border.
    let inner = inner_area(area);

    if app.first_run_suggestions.is_empty() {
        render_empty(f, app, inner);
    } else {
        render_suggestions(f, app, inner);
    }
}

// ── Inner renderers ───────────────────────────────────────────────────────────

/// Render the "no suggestions" fallback message.
fn render_empty(f: &mut Frame, app: &App, area: Rect) {
    let p = &app.palette;

    let text =
        "No repositories to suggest yet. Press `a` to add one manually, or `Esc` to continue.";
    let paragraph =
        Paragraph::new(Span::styled(text, Style::default().fg(p.dim))).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

/// Render the full suggestion list with header, blurb, list rows, and footer.
///
/// Layout (top to bottom inside the inner area):
/// 1. Blurb (2 dim lines).
/// 2. Empty separator row.
/// 3. Suggestion list rows (remaining height minus 2 for footer separator +
///    footer itself).
/// 4. Empty separator row.
/// 5. Footer hint line.
fn render_suggestions(f: &mut Frame, app: &App, area: Rect) {
    // Fixed row allocations: blurb (2) + blank (1) + footer-blank (1) + footer (1) = 5.
    // Everything else goes to the list.
    const BLURB_ROWS: u16 = 2;
    const SEPARATOR: u16 = 1;
    const FOOTER_ROWS: u16 = 1;

    let p = &app.palette;
    let fixed = BLURB_ROWS + SEPARATOR + SEPARATOR + FOOTER_ROWS;
    let list_height = area.height.saturating_sub(fixed);

    let [blurb_area, _sep1, list_area, _sep2, footer_area] = Layout::vertical([
        Constraint::Length(BLURB_ROWS),
        Constraint::Length(SEPARATOR),
        Constraint::Length(list_height),
        Constraint::Length(SEPARATOR),
        Constraint::Length(FOOTER_ROWS),
    ])
    .areas(area);

    // ── Blurb ─────────────────────────────────────────────────────────────────
    let blurb_lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "We found these repositories you're active in.",
            Style::default().fg(p.dim),
        )),
        Line::from(Span::styled("Pick the ones you want to track.", Style::default().fg(p.dim))),
    ];
    let blurb = Paragraph::new(blurb_lines);
    f.render_widget(blurb, blurb_area);

    // ── Suggestion list ───────────────────────────────────────────────────────
    let visible = list_height as usize;
    let total = app.first_run_suggestions.len();
    // Clamp cursor defensively — the state machine should keep it valid but
    // this guard prevents an index-out-of-bounds in the renderer.
    let cursor = app.first_run_cursor.min(total.saturating_sub(1));

    // Compute scroll offset so the cursor is always visible.
    let scroll = if visible == 0 {
        0
    } else {
        cursor.saturating_sub(visible - 1).min(total.saturating_sub(visible))
    };

    let list_lines: Vec<Line> = app
        .first_run_suggestions
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible)
        .map(|(idx, s)| {
            let checkbox = if s.selected { "[x]" } else { "[ ]" };
            let text = format!(
                " {checkbox} {}  ({} open item{})",
                s.repo,
                s.count,
                if s.count == 1 { "" } else { "s" }
            );
            if idx == cursor {
                Line::from(Span::styled(
                    text,
                    Style::default()
                        .fg(p.selection_fg)
                        .bg(p.selection_bg)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(text, Style::default().fg(p.foreground)))
            }
        })
        .collect();

    let list = Paragraph::new(list_lines);
    f.render_widget(list, list_area);

    // ── Footer ────────────────────────────────────────────────────────────────
    let footer = Paragraph::new(Span::styled(
        "Space toggle  Enter confirm  a add custom  Esc skip",
        Style::default().fg(p.dim),
    ));
    f.render_widget(footer, footer_area);
}

// ── Layout helpers ────────────────────────────────────────────────────────────

/// Return a centered overlay `Rect` (~70 cols wide, ~24 rows tall, clamped to
/// the terminal dimensions).
fn wizard_rect(area: Rect) -> Rect {
    // Cap width at 70, height at 24; never shrink below 8 rows so the content
    // has a fighting chance of rendering legibly on tiny terminals.
    let width = 70u16.min(area.width);
    let height = 24u16.min(area.height.saturating_sub(4)).max(8);

    let [_, center_v, _] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(height), Constraint::Fill(1)])
            .flex(Flex::Center)
            .areas(area);

    let [_, center_h, _] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(width), Constraint::Fill(1)])
            .flex(Flex::Center)
            .areas(center_v);

    center_h
}

/// Shrink `area` by 1 cell on each side to produce the usable inner region.
fn inner_area(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::app::{FirstRunSuggestion, Focus};

    fn make_app_with_suggestions(suggestions: Vec<FirstRunSuggestion>) -> App {
        let config = crate::config::Config::default();
        let session = crate::state::AppSession::default();
        let mut app = App::new(config, session);
        app.focus = Focus::FirstRun;
        app.first_run_suggestions = suggestions;
        app
    }

    /// Rendering the wizard with suggestions must show the suggestion list.
    #[test]
    fn draw_with_suggestions_shows_content() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let backend = ratatui::backend::TestBackend::new(80, 24);
            let mut terminal = ratatui::Terminal::new(backend).expect("test terminal");

            let suggestions = vec![
                FirstRunSuggestion { repo: "alice/foo".to_owned(), count: 3, selected: true },
                FirstRunSuggestion { repo: "bob/bar".to_owned(), count: 1, selected: false },
            ];
            let app = make_app_with_suggestions(suggestions);

            terminal.draw(|f| draw(f, &app)).expect("draw");

            let rendered: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect();

            assert!(rendered.contains("Welcome to octopeek"), "title must render");
            assert!(rendered.contains("alice/foo"), "suggestion repo must render");
        });
    }

    /// Rendering the wizard with no suggestions must show the fallback message.
    #[test]
    fn draw_empty_suggestions_shows_fallback() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::config::with_config_dir_override(tmp.path(), || {
            let backend = ratatui::backend::TestBackend::new(80, 24);
            let mut terminal = ratatui::Terminal::new(backend).expect("test terminal");

            let app = make_app_with_suggestions(vec![]);

            terminal.draw(|f| draw(f, &app)).expect("draw");

            let rendered: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect();

            assert!(
                rendered.contains("No repositories to suggest"),
                "fallback message must render; got: {rendered}"
            );
        });
    }
}
