//! Repo picker overlay for adding and removing watched repositories.
//!
//! Renders a centered modal with two sections:
//!
//! - **List mode** (default): shows currently tracked repos with a movable
//!   cursor.  `j`/`k` navigate; `d`/`Backspace` delete; `a`/`i` enter Input
//!   mode; `Enter` on a repo focuses that tab; `Esc` closes the picker.
//!
//! - **Input mode**: a text field at the bottom.  Typing appends characters;
//!   `Backspace` removes the last character; `Enter` validates and commits the
//!   repo slug; `Esc` returns to List mode.
//!
//! Any mutation (add / delete) is persisted immediately via `Config::save`.
//! When the picker closes, `App` syncs its `Tabs` to match `Config::repos`.
//!
//! # Validation
//!
//! [`is_valid_repo_slug`] is the single source of truth for what constitutes a
//! valid `owner/name` slug.  It is `pub` so tests and the app key-handler can
//! share the same rules without coupling to the UI module.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::App;

// ── Validation ────────────────────────────────────────────────────────────────

/// Maximum length of each half of an `owner/name` slug.
const MAX_HALF_LEN: usize = 100;

/// Return `true` when `s` is a well-formed `owner/name` repo slug.
///
/// Rules:
/// - Exactly one `/` separator.
/// - Both the owner and name halves are non-empty and at most 100 characters.
/// - Both halves contain only ASCII alphanumerics, `-`, `.`, or `_`.
///
/// # Examples
///
/// ```
/// use octopeek::ui::repo_picker::is_valid_repo_slug;
///
/// assert!(is_valid_repo_slug("rust-lang/rust"));
/// assert!(is_valid_repo_slug("owner_1/my.repo-name"));
/// assert!(!is_valid_repo_slug("no-slash"));
/// assert!(!is_valid_repo_slug("two//slashes"));
/// assert!(!is_valid_repo_slug("owner/"));
/// assert!(!is_valid_repo_slug(""));
/// ```
pub fn is_valid_repo_slug(s: &str) -> bool {
    // Must have exactly one '/'.
    let mut parts = s.splitn(3, '/');
    let owner = match parts.next() {
        Some(o) if !o.is_empty() => o,
        _ => return false,
    };
    let name = match parts.next() {
        Some(n) if !n.is_empty() => n,
        _ => return false,
    };
    // A third segment means there were two or more slashes.
    if parts.next().is_some() {
        return false;
    }

    // Length limits.
    if owner.len() > MAX_HALF_LEN || name.len() > MAX_HALF_LEN {
        return false;
    }

    // Character allow-list.
    let is_allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_');
    owner.chars().all(is_allowed) && name.chars().all(is_allowed)
}

// ── Rendering ─────────────────────────────────────────────────────────────────

/// Render the repo picker overlay centered in the terminal.
///
/// The caller is responsible for drawing this **after** all other widgets so
/// the overlay floats on top.
pub fn draw(f: &mut Frame, app: &App) {
    let p = &app.palette;
    let area = picker_rect(f.area());

    let block = Block::default()
        .title(" Repositories ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.border_focused))
        .style(Style::default().bg(p.help_bg));

    f.render_widget(Clear, area);
    f.render_widget(block, area);

    // Inner area with 1-cell padding on all sides.
    let inner = inner_area(area);

    // Split inner area into list section and input section.
    // Leave 3 rows at the bottom for the input field + label.
    let input_height: u16 = 3;
    let list_height = inner.height.saturating_sub(input_height);

    let [list_area, input_area] =
        Layout::vertical([Constraint::Length(list_height), Constraint::Length(input_height)])
            .areas(inner);

    render_list(f, app, list_area);
    render_input(f, app, input_area);
}

/// Render the repo list in `area`.
fn render_list(f: &mut Frame, app: &App, area: Rect) {
    let p = &app.palette;

    if app.config.repos.is_empty() {
        let hint = Paragraph::new(Line::from(Span::styled(
            "No repositories tracked yet.  Press `a` to add one.",
            Style::default().fg(p.dim),
        )))
        .wrap(Wrap { trim: false });
        f.render_widget(hint, area);
        return;
    }

    // Determine the visible window so the selected item is always shown.
    let visible_rows = area.height as usize;
    let total = app.config.repos.len();
    let cursor = app.repo_picker_list_cursor.min(total.saturating_sub(1));

    // Compute scroll offset so `cursor` is always in view.
    let scroll_offset = if visible_rows == 0 {
        0
    } else {
        cursor.saturating_sub(visible_rows - 1).min(total.saturating_sub(visible_rows))
    };

    let lines: Vec<Line> = app
        .config
        .repos
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_rows)
        .map(|(idx, repo)| {
            let selected = idx == cursor;
            let bullet = if selected { ">" } else { " " };
            let style = if selected {
                Style::default().fg(p.selection_fg).bg(p.selection_bg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(p.foreground)
            };
            Line::from(Span::styled(format!(" {bullet} {repo}"), style))
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

/// Render the input field in `area`.
fn render_input(f: &mut Frame, app: &App, area: Rect) {
    let p = &app.palette;
    let is_input_mode = app.repo_picker_mode == crate::app::RepoPickerMode::Input;

    let label_style = if is_input_mode {
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.dim)
    };

    let cursor_char = if is_input_mode { "_" } else { "" };
    let input_text = format!("{}{}", app.repo_picker_input, cursor_char);

    let border_style = if is_input_mode {
        Style::default().fg(p.border_focused)
    } else {
        Style::default().fg(p.border)
    };

    let block = Block::default()
        .title(Span::styled(" Add (owner/name): ", label_style))
        .borders(Borders::TOP)
        .border_style(border_style);

    let paragraph = Paragraph::new(Line::from(Span::styled(
        format!(" {input_text}"),
        Style::default().fg(p.foreground),
    )))
    .block(block);

    f.render_widget(paragraph, area);
}

// ── Layout helpers ─────────────────────────────────────────────────────────────

/// Return the centered overlay `Rect` (~60 cols wide).
fn picker_rect(area: Rect) -> Rect {
    let width = 62u16.min(area.width);
    // Height: up to 20 rows, but no more than the terminal height minus 4.
    let height = 20u16.min(area.height.saturating_sub(4)).max(8);

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

/// Shrink `area` by 1 cell on each side.
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
mod tests {
    use super::*;

    // ── is_valid_repo_slug ────────────────────────────────────────────────────

    #[test]
    fn valid_slugs_accepted() {
        assert!(is_valid_repo_slug("rust-lang/rust"));
        assert!(is_valid_repo_slug("owner/repo"));
        assert!(is_valid_repo_slug("my-org/my.repo_name-123"));
        assert!(is_valid_repo_slug("a/b"));
        assert!(is_valid_repo_slug("A/B")); // uppercase is allowed
    }

    #[test]
    fn empty_slug_rejected() {
        assert!(!is_valid_repo_slug(""));
    }

    #[test]
    fn no_slash_rejected() {
        assert!(!is_valid_repo_slug("no-slash"));
    }

    #[test]
    fn two_slashes_rejected() {
        assert!(!is_valid_repo_slug("owner/sub/repo"));
        assert!(!is_valid_repo_slug("owner//repo"));
    }

    #[test]
    fn empty_owner_rejected() {
        assert!(!is_valid_repo_slug("/name"));
    }

    #[test]
    fn empty_name_rejected() {
        assert!(!is_valid_repo_slug("owner/"));
    }

    #[test]
    fn bad_chars_rejected() {
        assert!(!is_valid_repo_slug("owner/repo name")); // space
        assert!(!is_valid_repo_slug("owner/repo!")); // exclamation
        assert!(!is_valid_repo_slug("owner@org/repo")); // at-sign in owner
    }

    #[test]
    fn too_long_owner_rejected() {
        let long = "a".repeat(101);
        assert!(!is_valid_repo_slug(&format!("{long}/repo")));
    }

    #[test]
    fn too_long_name_rejected() {
        let long = "a".repeat(101);
        assert!(!is_valid_repo_slug(&format!("owner/{long}")));
    }

    #[test]
    fn exactly_max_len_accepted() {
        let exactly = "a".repeat(100);
        assert!(is_valid_repo_slug(&format!("{exactly}/{exactly}")));
    }
}
