//! Color theme definitions for the octopeek TUI.
//!
//! Each theme maps to a [`Palette`] of concrete colors threaded through every
//! renderer. Many palette fields and helper methods are defined up-front for
//! later phases (markdown rendering in Phase 4, settings panel in Phase 4).
//! The module-level `allow(dead_code)` acknowledges that; per-item allows
//! would need ~40 attributes across the file.
#![allow(dead_code)]

use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

/// Selectable color themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Default,
    Dracula,
    SolarizedDark,
    SolarizedLight,
    Nord,
    GruvboxDark,
    GruvboxLight,
    GithubLight,
}

impl Theme {
    /// All selectable themes in display order.
    pub const ALL: &'static [Theme] = &[
        Theme::Default,
        Theme::Dracula,
        Theme::SolarizedDark,
        Theme::SolarizedLight,
        Theme::Nord,
        Theme::GruvboxDark,
        Theme::GruvboxLight,
        Theme::GithubLight,
    ];

    /// Human-readable display name for the theme.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Default => "Default",
            Theme::Dracula => "Dracula",
            Theme::SolarizedDark => "Solarized Dark",
            Theme::SolarizedLight => "Solarized Light",
            Theme::Nord => "Nord",
            Theme::GruvboxDark => "Gruvbox Dark",
            Theme::GruvboxLight => "Gruvbox Light",
            Theme::GithubLight => "GitHub Light",
        }
    }

    /// Name of the bundled syntect theme to use for syntax highlighting when
    /// this UI theme is active.
    ///
    /// The returned string is always a key present in
    /// [`syntect::highlighting::ThemeSet::load_defaults`]'s output.
    ///
    /// The exhaustive match (no `_` wildcard) ensures the compiler forces an
    /// update here whenever a new [`Theme`] variant is added.
    pub fn syntax_theme_name(self) -> &'static str {
        match self {
            Theme::Default | Theme::SolarizedDark | Theme::Nord => "base16-ocean.dark",
            Theme::Dracula | Theme::GruvboxDark => "base16-eighties.dark",
            Theme::SolarizedLight | Theme::GruvboxLight | Theme::GithubLight => "InspiredGitHub",
        }
    }
}

/// Concrete color values for one theme, threaded through every renderer.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub foreground: Color,
    pub dim: Color,
    pub border: Color,
    pub border_focused: Color,
    pub accent: Color,
    pub accent_alt: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    /// Foreground color for text rendered on an `accent`-colored background.
    ///
    /// For most themes `selection_fg` contrasts adequately with `accent`.
    /// GitHub Light is the exception: its `selection_fg` is the same as
    /// `accent`, which would produce invisible text on the accent background.
    pub on_accent_fg: Color,
    pub title: Color,
    pub h1: Color,
    pub h2: Color,
    pub h3: Color,
    pub heading_other: Color,
    pub inline_code: Color,
    pub code_fg: Color,
    pub code_bg: Color,
    pub code_border: Color,
    pub link: Color,
    pub list_marker: Color,
    pub task_marker: Color,
    pub block_quote_fg: Color,
    pub block_quote_border: Color,
    pub table_header: Color,
    pub table_border: Color,
    pub search_match_bg: Color,
    pub current_match_bg: Color,
    pub match_fg: Color,
    pub gutter: Color,
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,
    pub help_bg: Color,
    pub git_new: Color,
    pub git_modified: Color,
    /// Color for items needing the viewer's attention.
    pub needs_action: Color,
    /// Color for success states (CI passing, clean PR).
    pub success: Color,
    /// Color for non-critical warnings (branch behind, stale).
    pub warning: Color,
    /// Color for blocking issues (conflict, CI failure).
    pub danger: Color,
    /// Color for muted / secondary information.
    pub muted: Color,
}

impl Palette {
    /// Construct the color palette for the given theme.
    #[allow(clippy::too_many_lines)]
    pub fn from_theme(theme: Theme) -> Self {
        match theme {
            Theme::Default => Self {
                background: Color::Rgb(20, 20, 30),
                foreground: Color::Rgb(220, 220, 220),
                dim: Color::DarkGray,
                border: Color::DarkGray,
                border_focused: Color::Cyan,
                accent: Color::Cyan,
                accent_alt: Color::Yellow,
                selection_bg: Color::Rgb(0, 160, 80),
                selection_fg: Color::Black,
                on_accent_fg: Color::Black,
                title: Color::Rgb(220, 220, 220),
                h1: Color::Cyan,
                h2: Color::Blue,
                h3: Color::Magenta,
                heading_other: Color::White,
                inline_code: Color::Green,
                code_fg: Color::Rgb(180, 200, 180),
                code_bg: Color::Rgb(40, 40, 40),
                code_border: Color::DarkGray,
                link: Color::Blue,
                list_marker: Color::Yellow,
                task_marker: Color::Cyan,
                block_quote_fg: Color::Gray,
                block_quote_border: Color::DarkGray,
                table_header: Color::Cyan,
                table_border: Color::DarkGray,
                search_match_bg: Color::Yellow,
                current_match_bg: Color::Rgb(255, 120, 0),
                match_fg: Color::Black,
                gutter: Color::DarkGray,
                status_bar_bg: Color::Rgb(30, 30, 30),
                status_bar_fg: Color::Gray,
                // Lifted ~10 per channel above `background` so help, repo picker,
                // and confirm overlays are visibly distinct from the dashboard
                // behind them. Matching `background` would make the overlay
                // border the only visible cue — easily missed.
                help_bg: Color::Rgb(32, 32, 45),
                git_new: Color::Rgb(80, 200, 120),
                git_modified: Color::Rgb(220, 180, 60),
                needs_action: Color::Rgb(255, 200, 60),
                success: Color::Rgb(80, 200, 120),
                warning: Color::Rgb(220, 160, 40),
                danger: Color::Rgb(220, 60, 60),
                muted: Color::DarkGray,
            },
            Theme::Dracula => Self {
                // Official Dracula palette: https://draculatheme.com/contribute
                background: Color::Rgb(40, 42, 54),
                foreground: Color::Rgb(248, 248, 242),
                dim: Color::Rgb(98, 114, 164),
                border: Color::Rgb(68, 71, 90),
                border_focused: Color::Rgb(189, 147, 249),
                accent: Color::Rgb(189, 147, 249),
                accent_alt: Color::Rgb(241, 250, 140),
                selection_bg: Color::Rgb(68, 71, 90),
                selection_fg: Color::Rgb(248, 248, 242),
                on_accent_fg: Color::Rgb(248, 248, 242),
                title: Color::Rgb(248, 248, 242),
                h1: Color::Rgb(255, 121, 198),
                h2: Color::Rgb(189, 147, 249),
                h3: Color::Rgb(80, 250, 123),
                heading_other: Color::Rgb(248, 248, 242),
                inline_code: Color::Rgb(80, 250, 123),
                code_fg: Color::Rgb(248, 248, 242),
                code_bg: Color::Rgb(40, 42, 54),
                code_border: Color::Rgb(98, 114, 164),
                link: Color::Rgb(139, 233, 253),
                list_marker: Color::Rgb(241, 250, 140),
                task_marker: Color::Rgb(80, 250, 123),
                block_quote_fg: Color::Rgb(98, 114, 164),
                block_quote_border: Color::Rgb(98, 114, 164),
                table_header: Color::Rgb(255, 121, 198),
                table_border: Color::Rgb(98, 114, 164),
                search_match_bg: Color::Rgb(241, 250, 140),
                current_match_bg: Color::Rgb(255, 121, 198),
                match_fg: Color::Rgb(40, 42, 54),
                gutter: Color::Rgb(98, 114, 164),
                status_bar_bg: Color::Rgb(40, 42, 54),
                status_bar_fg: Color::Rgb(98, 114, 164),
                // Dracula's `current-line` color — brighter than the base
                // background so overlays are clearly distinct from the
                // dashboard behind them.
                help_bg: Color::Rgb(68, 71, 90),
                git_new: Color::Rgb(80, 250, 123),
                git_modified: Color::Rgb(241, 250, 140),
                needs_action: Color::Rgb(241, 250, 140),
                success: Color::Rgb(80, 250, 123),
                warning: Color::Rgb(255, 184, 108),
                danger: Color::Rgb(255, 85, 85),
                muted: Color::Rgb(98, 114, 164),
            },
            Theme::SolarizedDark => Self {
                // Ethan Schoonover's Solarized Dark: https://ethanschoonover.com/solarized/
                background: Color::Rgb(0, 43, 54),
                foreground: Color::Rgb(131, 148, 150),
                dim: Color::Rgb(88, 110, 117),
                border: Color::Rgb(88, 110, 117),
                border_focused: Color::Rgb(38, 139, 210),
                accent: Color::Rgb(38, 139, 210),
                accent_alt: Color::Rgb(181, 137, 0),
                selection_bg: Color::Rgb(7, 54, 66),
                selection_fg: Color::Rgb(147, 161, 161),
                on_accent_fg: Color::Rgb(147, 161, 161),
                title: Color::Rgb(147, 161, 161),
                h1: Color::Rgb(203, 75, 22),
                h2: Color::Rgb(38, 139, 210),
                h3: Color::Rgb(42, 161, 152),
                heading_other: Color::Rgb(131, 148, 150),
                inline_code: Color::Rgb(133, 153, 0),
                code_fg: Color::Rgb(131, 148, 150),
                code_bg: Color::Rgb(7, 54, 66),
                code_border: Color::Rgb(88, 110, 117),
                link: Color::Rgb(38, 139, 210),
                list_marker: Color::Rgb(181, 137, 0),
                task_marker: Color::Rgb(42, 161, 152),
                block_quote_fg: Color::Rgb(88, 110, 117),
                block_quote_border: Color::Rgb(88, 110, 117),
                table_header: Color::Rgb(203, 75, 22),
                table_border: Color::Rgb(88, 110, 117),
                search_match_bg: Color::Rgb(181, 137, 0),
                current_match_bg: Color::Rgb(203, 75, 22),
                match_fg: Color::Rgb(0, 43, 54),
                gutter: Color::Rgb(88, 110, 117),
                status_bar_bg: Color::Rgb(7, 54, 66),
                status_bar_fg: Color::Rgb(88, 110, 117),
                help_bg: Color::Rgb(7, 54, 66),
                git_new: Color::Rgb(133, 153, 0),
                git_modified: Color::Rgb(181, 137, 0),
                needs_action: Color::Rgb(181, 137, 0),
                success: Color::Rgb(133, 153, 0),
                warning: Color::Rgb(203, 75, 22),
                danger: Color::Rgb(220, 50, 47),
                muted: Color::Rgb(88, 110, 117),
            },
            Theme::SolarizedLight => Self {
                // Ethan Schoonover's Solarized Light: https://ethanschoonover.com/solarized/
                background: Color::Rgb(253, 246, 227),
                foreground: Color::Rgb(101, 123, 131),
                dim: Color::Rgb(147, 161, 161),
                border: Color::Rgb(238, 232, 213),
                border_focused: Color::Rgb(38, 139, 210),
                accent: Color::Rgb(38, 139, 210),
                accent_alt: Color::Rgb(181, 137, 0),
                selection_bg: Color::Rgb(238, 232, 213),
                selection_fg: Color::Rgb(88, 110, 117),
                on_accent_fg: Color::Rgb(253, 246, 227),
                title: Color::Rgb(88, 110, 117),
                h1: Color::Rgb(203, 75, 22),
                h2: Color::Rgb(38, 139, 210),
                h3: Color::Rgb(42, 161, 152),
                heading_other: Color::Rgb(88, 110, 117),
                inline_code: Color::Rgb(133, 153, 0),
                code_fg: Color::Rgb(101, 123, 131),
                code_bg: Color::Rgb(238, 232, 213),
                code_border: Color::Rgb(147, 161, 161),
                link: Color::Rgb(38, 139, 210),
                list_marker: Color::Rgb(181, 137, 0),
                task_marker: Color::Rgb(42, 161, 152),
                block_quote_fg: Color::Rgb(147, 161, 161),
                block_quote_border: Color::Rgb(147, 161, 161),
                table_header: Color::Rgb(203, 75, 22),
                table_border: Color::Rgb(147, 161, 161),
                search_match_bg: Color::Rgb(181, 137, 0),
                current_match_bg: Color::Rgb(203, 75, 22),
                match_fg: Color::Rgb(253, 246, 227),
                gutter: Color::Rgb(147, 161, 161),
                status_bar_bg: Color::Rgb(238, 232, 213),
                status_bar_fg: Color::Rgb(101, 123, 131),
                help_bg: Color::Rgb(238, 232, 213),
                git_new: Color::Rgb(133, 153, 0),
                git_modified: Color::Rgb(181, 137, 0),
                needs_action: Color::Rgb(181, 137, 0),
                success: Color::Rgb(133, 153, 0),
                warning: Color::Rgb(203, 75, 22),
                danger: Color::Rgb(220, 50, 47),
                muted: Color::Rgb(147, 161, 161),
            },
            Theme::Nord => Self {
                // Arctic, north-bluish color palette: https://www.nordtheme.com/docs/colors-and-palettes
                background: Color::Rgb(46, 52, 64),
                foreground: Color::Rgb(216, 222, 233),
                dim: Color::Rgb(76, 86, 106),
                border: Color::Rgb(67, 76, 94),
                border_focused: Color::Rgb(136, 192, 208),
                accent: Color::Rgb(136, 192, 208),
                accent_alt: Color::Rgb(235, 203, 139),
                selection_bg: Color::Rgb(67, 76, 94),
                selection_fg: Color::Rgb(236, 239, 244),
                on_accent_fg: Color::Rgb(46, 52, 64),
                title: Color::Rgb(236, 239, 244),
                h1: Color::Rgb(191, 97, 106),
                h2: Color::Rgb(136, 192, 208),
                h3: Color::Rgb(163, 190, 140),
                heading_other: Color::Rgb(216, 222, 233),
                inline_code: Color::Rgb(163, 190, 140),
                code_fg: Color::Rgb(216, 222, 233),
                code_bg: Color::Rgb(59, 66, 82),
                code_border: Color::Rgb(76, 86, 106),
                link: Color::Rgb(129, 161, 193),
                list_marker: Color::Rgb(235, 203, 139),
                task_marker: Color::Rgb(163, 190, 140),
                block_quote_fg: Color::Rgb(76, 86, 106),
                block_quote_border: Color::Rgb(76, 86, 106),
                table_header: Color::Rgb(94, 129, 172),
                table_border: Color::Rgb(76, 86, 106),
                search_match_bg: Color::Rgb(235, 203, 139),
                current_match_bg: Color::Rgb(191, 97, 106),
                match_fg: Color::Rgb(46, 52, 64),
                gutter: Color::Rgb(76, 86, 106),
                status_bar_bg: Color::Rgb(59, 66, 82),
                status_bar_fg: Color::Rgb(76, 86, 106),
                help_bg: Color::Rgb(59, 66, 82),
                git_new: Color::Rgb(163, 190, 140),
                git_modified: Color::Rgb(235, 203, 139),
                needs_action: Color::Rgb(235, 203, 139),
                success: Color::Rgb(163, 190, 140),
                warning: Color::Rgb(208, 135, 112),
                danger: Color::Rgb(191, 97, 106),
                muted: Color::Rgb(76, 86, 106),
            },
            Theme::GruvboxDark => Self {
                // Gruvbox Dark: https://github.com/morhetz/gruvbox
                background: Color::Rgb(40, 40, 40),
                foreground: Color::Rgb(235, 219, 178),
                dim: Color::Rgb(146, 131, 116),
                border: Color::Rgb(80, 73, 69),
                border_focused: Color::Rgb(214, 93, 14),
                accent: Color::Rgb(250, 189, 47),
                accent_alt: Color::Rgb(184, 187, 38),
                selection_bg: Color::Rgb(80, 73, 69),
                selection_fg: Color::Rgb(235, 219, 178),
                on_accent_fg: Color::Rgb(40, 40, 40),
                title: Color::Rgb(235, 219, 178),
                h1: Color::Rgb(251, 73, 52),
                h2: Color::Rgb(250, 189, 47),
                h3: Color::Rgb(184, 187, 38),
                heading_other: Color::Rgb(235, 219, 178),
                inline_code: Color::Rgb(184, 187, 38),
                code_fg: Color::Rgb(235, 219, 178),
                code_bg: Color::Rgb(50, 48, 47),
                code_border: Color::Rgb(80, 73, 69),
                link: Color::Rgb(131, 165, 152),
                list_marker: Color::Rgb(250, 189, 47),
                task_marker: Color::Rgb(184, 187, 38),
                block_quote_fg: Color::Rgb(146, 131, 116),
                block_quote_border: Color::Rgb(146, 131, 116),
                table_header: Color::Rgb(214, 93, 14),
                table_border: Color::Rgb(80, 73, 69),
                search_match_bg: Color::Rgb(250, 189, 47),
                current_match_bg: Color::Rgb(251, 73, 52),
                match_fg: Color::Rgb(40, 40, 40),
                gutter: Color::Rgb(102, 92, 84),
                status_bar_bg: Color::Rgb(50, 48, 47),
                status_bar_fg: Color::Rgb(146, 131, 116),
                help_bg: Color::Rgb(50, 48, 47),
                git_new: Color::Rgb(184, 187, 38),
                git_modified: Color::Rgb(250, 189, 47),
                needs_action: Color::Rgb(250, 189, 47),
                success: Color::Rgb(184, 187, 38),
                warning: Color::Rgb(214, 93, 14),
                danger: Color::Rgb(251, 73, 52),
                muted: Color::Rgb(146, 131, 116),
            },
            Theme::GruvboxLight => Self {
                // Gruvbox Light: https://github.com/morhetz/gruvbox
                background: Color::Rgb(251, 241, 199),
                foreground: Color::Rgb(60, 56, 54),
                dim: Color::Rgb(146, 131, 116),
                border: Color::Rgb(213, 196, 161),
                border_focused: Color::Rgb(214, 93, 14),
                accent: Color::Rgb(215, 153, 33),
                accent_alt: Color::Rgb(152, 151, 26),
                selection_bg: Color::Rgb(235, 219, 178),
                selection_fg: Color::Rgb(60, 56, 54),
                on_accent_fg: Color::Rgb(60, 56, 54),
                title: Color::Rgb(60, 56, 54),
                h1: Color::Rgb(204, 36, 29),
                h2: Color::Rgb(215, 153, 33),
                h3: Color::Rgb(152, 151, 26),
                heading_other: Color::Rgb(60, 56, 54),
                inline_code: Color::Rgb(177, 98, 134),
                code_fg: Color::Rgb(60, 56, 54),
                code_bg: Color::Rgb(235, 219, 178),
                code_border: Color::Rgb(213, 196, 161),
                link: Color::Rgb(69, 133, 136),
                list_marker: Color::Rgb(215, 153, 33),
                task_marker: Color::Rgb(104, 157, 106),
                block_quote_fg: Color::Rgb(146, 131, 116),
                block_quote_border: Color::Rgb(213, 196, 161),
                table_header: Color::Rgb(214, 93, 14),
                table_border: Color::Rgb(213, 196, 161),
                search_match_bg: Color::Rgb(215, 153, 33),
                current_match_bg: Color::Rgb(214, 93, 14),
                match_fg: Color::Rgb(251, 241, 199),
                gutter: Color::Rgb(146, 131, 116),
                status_bar_bg: Color::Rgb(235, 219, 178),
                status_bar_fg: Color::Rgb(80, 73, 69),
                help_bg: Color::Rgb(235, 219, 178),
                git_new: Color::Rgb(152, 151, 26),
                git_modified: Color::Rgb(215, 153, 33),
                needs_action: Color::Rgb(215, 153, 33),
                success: Color::Rgb(104, 157, 106),
                warning: Color::Rgb(214, 93, 14),
                danger: Color::Rgb(204, 36, 29),
                muted: Color::Rgb(146, 131, 116),
            },
            Theme::GithubLight => Self {
                // GitHub Light: https://primer.style/primitives/colors
                background: Color::Rgb(255, 255, 255),
                foreground: Color::Rgb(31, 35, 40),
                dim: Color::Rgb(101, 109, 118),
                border: Color::Rgb(208, 215, 222),
                border_focused: Color::Rgb(9, 105, 218),
                accent: Color::Rgb(9, 105, 218),
                accent_alt: Color::Rgb(154, 103, 0),
                selection_bg: Color::Rgb(221, 244, 255),
                selection_fg: Color::Rgb(9, 105, 218),
                // White on the vivid blue — selection_fg is also #0969da which would
                // produce invisible blue-on-blue text if used on an accent background.
                on_accent_fg: Color::Rgb(255, 255, 255),
                title: Color::Rgb(31, 35, 40),
                h1: Color::Rgb(9, 105, 218),
                h2: Color::Rgb(154, 103, 0),
                h3: Color::Rgb(26, 127, 55),
                heading_other: Color::Rgb(31, 35, 40),
                inline_code: Color::Rgb(207, 34, 46),
                code_fg: Color::Rgb(31, 35, 40),
                code_bg: Color::Rgb(246, 248, 250),
                code_border: Color::Rgb(208, 215, 222),
                link: Color::Rgb(9, 105, 218),
                list_marker: Color::Rgb(154, 103, 0),
                task_marker: Color::Rgb(26, 127, 55),
                block_quote_fg: Color::Rgb(101, 109, 118),
                block_quote_border: Color::Rgb(208, 215, 222),
                table_header: Color::Rgb(9, 105, 218),
                table_border: Color::Rgb(208, 215, 222),
                search_match_bg: Color::Rgb(255, 211, 61),
                current_match_bg: Color::Rgb(255, 143, 0),
                match_fg: Color::Rgb(31, 35, 40),
                gutter: Color::Rgb(101, 109, 118),
                status_bar_bg: Color::Rgb(246, 248, 250),
                status_bar_fg: Color::Rgb(101, 109, 118),
                help_bg: Color::Rgb(246, 248, 250),
                git_new: Color::Rgb(26, 127, 55),
                git_modified: Color::Rgb(154, 103, 0),
                needs_action: Color::Rgb(154, 103, 0),
                success: Color::Rgb(26, 127, 55),
                warning: Color::Rgb(130, 80, 0),
                danger: Color::Rgb(207, 34, 46),
                muted: Color::Rgb(101, 109, 118),
            },
        }
    }

    /// Style for unfocused panel borders.
    pub fn border_style(self) -> Style {
        Style::new().fg(self.border)
    }

    /// Style for focused panel borders.
    pub fn border_focused_style(self) -> Style {
        Style::new().fg(self.border_focused)
    }

    /// Bold style for widget titles.
    pub fn title_style(self) -> Style {
        Style::new().fg(self.title).add_modifier(Modifier::BOLD)
    }

    /// Style for the currently selected list item.
    pub fn selected_style(self) -> Style {
        Style::new().bg(self.selection_bg).fg(self.selection_fg).add_modifier(Modifier::BOLD)
    }

    /// Style for de-emphasized (dim) text.
    pub fn dim_style(self) -> Style {
        Style::new().fg(self.dim)
    }

    /// Map a [`crate::ui::glyphs::ColorRole`] to a concrete [`Color`].
    pub fn color_for(self, role: crate::ui::glyphs::ColorRole) -> Color {
        use crate::ui::glyphs::ColorRole;
        match role {
            ColorRole::NeedsAction => self.needs_action,
            ColorRole::Success => self.success,
            ColorRole::Warning => self.warning,
            ColorRole::Danger => self.danger,
            ColorRole::Muted => self.muted,
            ColorRole::Accent => self.accent,
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::from_theme(Theme::Default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every theme must have `on_accent_fg != accent` so that text drawn on an
    /// accent-coloured background is never invisible.
    #[test]
    fn on_accent_fg_contrasts_with_accent() {
        for &theme in Theme::ALL {
            let p = Palette::from_theme(theme);
            assert_ne!(
                p.on_accent_fg, p.accent,
                "Theme {theme:?}: on_accent_fg == accent — text would be invisible",
            );
        }
    }

    /// All theme labels must be non-empty strings.
    #[test]
    fn all_themes_have_labels() {
        for &theme in Theme::ALL {
            assert!(!theme.label().is_empty(), "Theme {theme:?} has empty label");
        }
    }

    /// `syntax_theme_name` must return one of the three known syntect defaults.
    #[test]
    fn syntax_theme_name_is_valid() {
        const VALID: &[&str] = &["base16-ocean.dark", "base16-eighties.dark", "InspiredGitHub"];
        for &theme in Theme::ALL {
            assert!(
                VALID.contains(&theme.syntax_theme_name()),
                "Theme {theme:?}: unknown syntax theme name",
            );
        }
    }
}
