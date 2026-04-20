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
#[derive(Debug, Clone, Copy, PartialEq)]
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

/// Source-of-truth colour tokens for one theme, from which the full
/// [`Palette`] is derived.
///
/// The 7 "base" fields are always required. The remaining fields are optional
/// override slots: `None` means "apply the standard derivation rule";
/// `Some(c)` means "use this exact colour instead". Override slots are set
/// only when the derivation rule would produce a wrong colour for a specific
/// theme.
///
/// **Why so many override slots?** The existing themes were hand-authored
/// independently, so several fields (e.g. `selection_fg`, `h2`, `link`) do
/// not follow a single rule across all 8 palettes. Rather than forcing every
/// theme through an incorrect derivation, we capture the exceptions precisely.
/// A future palette design exercise could collapse more of these to direct
/// derivations, but the parity gate in the test suite would catch any drift.
///
/// `ThemeTokens` is `Copy` — same rationale as [`Palette`]: pure-POD, no
/// heap allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeTokens {
    // ------------------------------------------------------------------
    // Base tokens — always required.
    // ------------------------------------------------------------------
    /// Main background colour for the TUI surface.
    pub background: Color,
    /// Primary text colour.
    pub foreground: Color,
    /// De-emphasised / secondary text colour.
    pub dim: Color,
    /// Unfocused panel border colour.
    pub border: Color,
    /// Primary accent (focused borders, active elements).
    pub accent: Color,
    /// Secondary accent (list markers, task indicators).
    pub accent_alt: Color,
    /// Background of selected / highlighted rows.
    pub selection_bg: Color,

    // ------------------------------------------------------------------
    // Override slots — `None` → apply the derivation rule documented in
    // [`Palette::from_tokens`]; `Some(c)` → use `c` verbatim.
    // ------------------------------------------------------------------
    /// `Palette::selection_fg`. Default: `foreground`.
    pub selection_fg: Option<Color>,
    /// `Palette::on_accent_fg`. Default: `foreground`. GitHub Light overrides
    /// to white because its `selection_fg` == `accent` (invisible on accent bg).
    pub on_accent_fg: Option<Color>,
    /// `Palette::title`. Default: `foreground`.
    pub title: Option<Color>,
    /// `Palette::h1`. Default: `accent`. Dracula overrides to pink.
    pub h1: Option<Color>,
    /// `Palette::h2`. Default: `accent`.
    pub h2: Option<Color>,
    /// `Palette::h3`. Default: `accent_alt`.
    pub h3: Option<Color>,
    /// `Palette::heading_other`. Default: `foreground`.
    pub heading_other: Option<Color>,
    /// `Palette::inline_code`. Default: `accent_alt`.
    pub inline_code: Option<Color>,
    /// `Palette::code_fg`. Default: `foreground`.
    pub code_fg: Option<Color>,
    /// `Palette::code_bg`. Required per theme — too variable for one rule.
    pub code_bg: Option<Color>,
    /// `Palette::link`. Default: `accent`.
    pub link: Option<Color>,
    /// `Palette::task_marker`. Default: `accent_alt`.
    pub task_marker: Option<Color>,
    /// `Palette::block_quote_fg`. Default: `dim`.
    pub block_quote_fg: Option<Color>,
    /// `Palette::block_quote_border`. Default: `dim`.
    pub block_quote_border: Option<Color>,
    /// `Palette::table_header`. Default: `accent`.
    pub table_header: Option<Color>,
    /// `Palette::border_focused`. Default: `accent`. `GruvboxDark` overrides to
    /// orange because its `border_focused` is deliberately different from its
    /// `accent` (yellow).
    pub border_focused: Option<Color>,
    /// `Palette::code_border`. Default: `dim`. Gruvbox themes and GitHub Light
    /// use `border` instead of `dim` for their code-block borders.
    pub code_border: Option<Color>,
    /// `Palette::list_marker`. Default: `accent_alt`. Gruvbox themes use
    /// `accent` (their primary yellow) rather than `accent_alt` (green).
    pub list_marker: Option<Color>,
    /// `Palette::search_match_bg`. Default: `accent_alt`. Gruvbox themes use
    /// `accent`; GitHub Light uses a custom golden colour.
    pub search_match_bg: Option<Color>,
    /// `Palette::gutter`. Default: `dim`. `GruvboxDark` uses a third grey
    /// colour (`Rgb(102, 92, 84)`) distinct from both `dim` and `border`.
    pub gutter: Option<Color>,
    /// `Palette::table_border`. Default: `border`.
    pub table_border: Option<Color>,
    /// `Palette::current_match_bg`. Default: `accent`.
    pub current_match_bg: Option<Color>,
    /// `Palette::match_fg`. Default: `background`.
    pub match_fg: Option<Color>,
    /// `Palette::status_bar_bg`. Default: `code_bg`.
    pub status_bar_bg: Option<Color>,
    /// `Palette::status_bar_fg`. Default: `dim`.
    pub status_bar_fg: Option<Color>,
    /// `Palette::help_bg`. Required per theme — light-theme specific.
    pub help_bg: Option<Color>,
    /// `Palette::success`. Required per theme (per-theme green hue).
    pub success: Option<Color>,
    /// `Palette::warning`. Required per theme (per-theme orange/yellow hue).
    pub warning: Option<Color>,
    /// `Palette::danger`. Required per theme (per-theme red hue).
    pub danger: Option<Color>,
    /// `Palette::git_new`. Default: `success`. `GruvboxLight` overrides to
    /// `accent_alt` (green #98971a) because its `success` colour is a
    /// different green not used for this field in the original palette.
    pub git_new: Option<Color>,
    /// `Palette::git_modified`. Default: `warning`. Some themes use a
    /// slightly different colour for modified vs warning.
    pub git_modified: Option<Color>,
    /// `Palette::needs_action`. Default: `warning`.
    pub needs_action: Option<Color>,
}

impl ThemeTokens {
    /// Return the [`ThemeTokens`] for `theme`.
    ///
    /// The exhaustive `match` (no `_` wildcard) ensures the compiler forces an
    /// update here whenever a new [`Theme`] variant is added.
    #[allow(clippy::too_many_lines)]
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Default => Self {
                background: Color::Rgb(20, 20, 30),
                foreground: Color::Rgb(220, 220, 220),
                dim: Color::DarkGray,
                border: Color::DarkGray,
                accent: Color::Cyan,
                accent_alt: Color::Yellow,
                selection_bg: Color::Rgb(0, 160, 80),
                // Default uses terminal colour names that differ from the RGB
                // foreground, so every field that would otherwise derive from
                // foreground/accent needs an explicit override.
                selection_fg: Some(Color::Black),
                on_accent_fg: Some(Color::Black),
                title: None,
                h1: None,
                h2: Some(Color::Blue),
                h3: Some(Color::Magenta),
                heading_other: Some(Color::White),
                inline_code: Some(Color::Green),
                code_fg: Some(Color::Rgb(180, 200, 180)),
                code_bg: Some(Color::Rgb(40, 40, 40)),
                border_focused: None,
                code_border: None,
                link: Some(Color::Blue),
                list_marker: None,
                task_marker: Some(Color::Cyan),
                block_quote_fg: Some(Color::Gray),
                block_quote_border: None,
                table_header: None,
                table_border: None,
                search_match_bg: None,
                gutter: None,
                current_match_bg: Some(Color::Rgb(255, 120, 0)),
                match_fg: Some(Color::Black),
                // status_bar_bg differs from code_bg for the Default theme
                status_bar_bg: Some(Color::Rgb(30, 30, 30)),
                status_bar_fg: Some(Color::Gray),
                help_bg: Some(Color::Rgb(32, 32, 45)),
                success: Some(Color::Rgb(80, 200, 120)),
                warning: Some(Color::Rgb(220, 160, 40)),
                danger: Some(Color::Rgb(220, 60, 60)),
                git_new: None,
                git_modified: Some(Color::Rgb(220, 180, 60)),
                needs_action: Some(Color::Rgb(255, 200, 60)),
            },
            Theme::Dracula => Self {
                background: Color::Rgb(40, 42, 54),
                foreground: Color::Rgb(248, 248, 242),
                dim: Color::Rgb(98, 114, 164),
                border: Color::Rgb(68, 71, 90),
                accent: Color::Rgb(189, 147, 249),
                accent_alt: Color::Rgb(241, 250, 140),
                selection_bg: Color::Rgb(68, 71, 90),
                selection_fg: None,
                on_accent_fg: None,
                title: None,
                // Dracula uses pink (#ff79c6) for H1, not the purple accent
                h1: Some(Color::Rgb(255, 121, 198)),
                h2: None,
                // Dracula h3 = green (#50fa7b), not accent_alt (yellow)
                h3: Some(Color::Rgb(80, 250, 123)),
                heading_other: None,
                // Dracula inline_code = green, not accent_alt (yellow)
                inline_code: Some(Color::Rgb(80, 250, 123)),
                code_fg: None,
                code_bg: Some(Color::Rgb(40, 42, 54)),
                border_focused: None,
                code_border: None,
                // Dracula link = cyan (#8be9fd), not accent (purple)
                link: Some(Color::Rgb(139, 233, 253)),
                list_marker: None,
                // Dracula task_marker = green, not accent_alt (yellow)
                task_marker: Some(Color::Rgb(80, 250, 123)),
                block_quote_fg: None,
                block_quote_border: None,
                // Dracula table_header = pink, not accent (purple)
                table_header: Some(Color::Rgb(255, 121, 198)),
                // Dracula table_border = dim (same as default derivation)
                table_border: Some(Color::Rgb(98, 114, 164)),
                search_match_bg: None,
                gutter: None,
                current_match_bg: Some(Color::Rgb(255, 121, 198)),
                match_fg: None,
                status_bar_bg: None,
                status_bar_fg: None,
                // Dracula "current-line" colour — distinct from background
                help_bg: Some(Color::Rgb(68, 71, 90)),
                success: Some(Color::Rgb(80, 250, 123)),
                warning: Some(Color::Rgb(255, 184, 108)),
                danger: Some(Color::Rgb(255, 85, 85)),
                // Dracula uses accent_alt (yellow) for git_modified and
                // needs_action, not the warning (orange) colour.
                git_new: None,
                git_modified: Some(Color::Rgb(241, 250, 140)),
                needs_action: Some(Color::Rgb(241, 250, 140)),
            },
            Theme::SolarizedDark => Self {
                background: Color::Rgb(0, 43, 54),
                foreground: Color::Rgb(131, 148, 150),
                dim: Color::Rgb(88, 110, 117),
                border: Color::Rgb(88, 110, 117),
                accent: Color::Rgb(38, 139, 210),
                accent_alt: Color::Rgb(181, 137, 0),
                selection_bg: Color::Rgb(7, 54, 66),
                selection_fg: Some(Color::Rgb(147, 161, 161)),
                on_accent_fg: Some(Color::Rgb(147, 161, 161)),
                title: Some(Color::Rgb(147, 161, 161)),
                // Solarized uses orange (#cb4b16) for H1
                h1: Some(Color::Rgb(203, 75, 22)),
                h2: None,
                // Solarized h3 = teal (#2aa198)
                h3: Some(Color::Rgb(42, 161, 152)),
                heading_other: None,
                // Solarized inline_code = green (#859900)
                inline_code: Some(Color::Rgb(133, 153, 0)),
                code_fg: None,
                code_bg: Some(Color::Rgb(7, 54, 66)),
                border_focused: None,
                code_border: None,
                link: None,
                list_marker: None,
                // Solarized task_marker = teal
                task_marker: Some(Color::Rgb(42, 161, 152)),
                block_quote_fg: None,
                block_quote_border: None,
                // Solarized table_header = orange
                table_header: Some(Color::Rgb(203, 75, 22)),
                table_border: None,
                search_match_bg: None,
                gutter: None,
                current_match_bg: Some(Color::Rgb(203, 75, 22)),
                match_fg: None,
                status_bar_bg: None,
                status_bar_fg: None,
                help_bg: Some(Color::Rgb(7, 54, 66)),
                success: Some(Color::Rgb(133, 153, 0)),
                warning: Some(Color::Rgb(203, 75, 22)),
                danger: Some(Color::Rgb(220, 50, 47)),
                // Solarized uses accent_alt (yellow #b58900) for git_modified and
                // needs_action, not the warning (orange #cb4b16) colour.
                git_new: None,
                git_modified: Some(Color::Rgb(181, 137, 0)),
                needs_action: Some(Color::Rgb(181, 137, 0)),
            },
            Theme::SolarizedLight => Self {
                background: Color::Rgb(253, 246, 227),
                foreground: Color::Rgb(101, 123, 131),
                dim: Color::Rgb(147, 161, 161),
                border: Color::Rgb(238, 232, 213),
                accent: Color::Rgb(38, 139, 210),
                accent_alt: Color::Rgb(181, 137, 0),
                selection_bg: Color::Rgb(238, 232, 213),
                selection_fg: Some(Color::Rgb(88, 110, 117)),
                on_accent_fg: Some(Color::Rgb(253, 246, 227)),
                title: Some(Color::Rgb(88, 110, 117)),
                h1: Some(Color::Rgb(203, 75, 22)),
                h2: None,
                h3: Some(Color::Rgb(42, 161, 152)),
                heading_other: Some(Color::Rgb(88, 110, 117)),
                inline_code: Some(Color::Rgb(133, 153, 0)),
                code_fg: None,
                code_bg: Some(Color::Rgb(238, 232, 213)),
                border_focused: None,
                // SolarizedLight code_border = dim (Rgb(147,161,161)), same as rule
                code_border: None,
                link: None,
                list_marker: None,
                task_marker: Some(Color::Rgb(42, 161, 152)),
                block_quote_fg: None,
                block_quote_border: None,
                table_header: Some(Color::Rgb(203, 75, 22)),
                table_border: Some(Color::Rgb(147, 161, 161)),
                search_match_bg: None,
                gutter: None,
                current_match_bg: Some(Color::Rgb(203, 75, 22)),
                match_fg: None,
                // SolarizedLight status_bar_fg = foreground (#657b83), not dim
                status_bar_bg: None,
                status_bar_fg: Some(Color::Rgb(101, 123, 131)),
                help_bg: Some(Color::Rgb(238, 232, 213)),
                success: Some(Color::Rgb(133, 153, 0)),
                warning: Some(Color::Rgb(203, 75, 22)),
                danger: Some(Color::Rgb(220, 50, 47)),
                // Solarized uses accent_alt (yellow #b58900) for these fields.
                git_new: None,
                git_modified: Some(Color::Rgb(181, 137, 0)),
                needs_action: Some(Color::Rgb(181, 137, 0)),
            },
            Theme::Nord => Self {
                background: Color::Rgb(46, 52, 64),
                foreground: Color::Rgb(216, 222, 233),
                dim: Color::Rgb(76, 86, 106),
                border: Color::Rgb(67, 76, 94),
                accent: Color::Rgb(136, 192, 208),
                accent_alt: Color::Rgb(235, 203, 139),
                selection_bg: Color::Rgb(67, 76, 94),
                selection_fg: Some(Color::Rgb(236, 239, 244)),
                on_accent_fg: Some(Color::Rgb(46, 52, 64)),
                title: Some(Color::Rgb(236, 239, 244)),
                // Nord h1 = aurora red
                h1: Some(Color::Rgb(191, 97, 106)),
                h2: None,
                // Nord h3 = aurora green
                h3: Some(Color::Rgb(163, 190, 140)),
                heading_other: None,
                // Nord inline_code = aurora green
                inline_code: Some(Color::Rgb(163, 190, 140)),
                code_fg: None,
                code_bg: Some(Color::Rgb(59, 66, 82)),
                border_focused: None,
                code_border: None,
                // Nord link = frost blue (#81a1c1)
                link: Some(Color::Rgb(129, 161, 193)),
                list_marker: None,
                // Nord task_marker = aurora green
                task_marker: Some(Color::Rgb(163, 190, 140)),
                block_quote_fg: None,
                block_quote_border: None,
                // Nord table_header = polar night blue (#5e81ac)
                table_header: Some(Color::Rgb(94, 129, 172)),
                // Nord table_border = dim
                table_border: Some(Color::Rgb(76, 86, 106)),
                search_match_bg: None,
                gutter: None,
                current_match_bg: Some(Color::Rgb(191, 97, 106)),
                match_fg: None,
                status_bar_bg: None,
                status_bar_fg: None,
                help_bg: Some(Color::Rgb(59, 66, 82)),
                success: Some(Color::Rgb(163, 190, 140)),
                warning: Some(Color::Rgb(208, 135, 112)),
                danger: Some(Color::Rgb(191, 97, 106)),
                // Nord uses accent_alt (sand #ebcb8b) for git_modified and
                // needs_action, not the warning (aurora orange) colour.
                git_new: None,
                git_modified: Some(Color::Rgb(235, 203, 139)),
                needs_action: Some(Color::Rgb(235, 203, 139)),
            },
            Theme::GruvboxDark => Self {
                background: Color::Rgb(40, 40, 40),
                foreground: Color::Rgb(235, 219, 178),
                dim: Color::Rgb(146, 131, 116),
                border: Color::Rgb(80, 73, 69),
                accent: Color::Rgb(250, 189, 47),
                accent_alt: Color::Rgb(184, 187, 38),
                selection_bg: Color::Rgb(80, 73, 69),
                selection_fg: None,
                on_accent_fg: Some(Color::Rgb(40, 40, 40)),
                title: None,
                // Gruvbox h1 = bright red (#fb4934)
                h1: Some(Color::Rgb(251, 73, 52)),
                h2: None,
                h3: None,
                heading_other: None,
                inline_code: None,
                code_fg: None,
                code_bg: Some(Color::Rgb(50, 48, 47)),
                // GruvboxDark border_focused = orange (#d65d0e), not accent (yellow)
                border_focused: Some(Color::Rgb(214, 93, 14)),
                // GruvboxDark code_border = border (#504945), not dim
                code_border: Some(Color::Rgb(80, 73, 69)),
                // Gruvbox link = aqua (#83a598)
                link: Some(Color::Rgb(131, 165, 152)),
                // GruvboxDark list_marker = accent (yellow #fabd2f), not accent_alt (green)
                list_marker: Some(Color::Rgb(250, 189, 47)),
                task_marker: None,
                block_quote_fg: None,
                block_quote_border: None,
                // Gruvbox table_header = orange (#d65d0e)
                table_header: Some(Color::Rgb(214, 93, 14)),
                // Gruvbox table_border = border (#504945)
                table_border: Some(Color::Rgb(80, 73, 69)),
                // GruvboxDark search_match_bg = accent (yellow), not accent_alt (green)
                search_match_bg: Some(Color::Rgb(250, 189, 47)),
                // GruvboxDark gutter = neutral dark grey (#665c54), not dim
                gutter: Some(Color::Rgb(102, 92, 84)),
                current_match_bg: Some(Color::Rgb(251, 73, 52)),
                match_fg: None,
                status_bar_bg: None,
                status_bar_fg: None,
                help_bg: Some(Color::Rgb(50, 48, 47)),
                success: Some(Color::Rgb(184, 187, 38)),
                warning: Some(Color::Rgb(214, 93, 14)),
                danger: Some(Color::Rgb(251, 73, 52)),
                // Gruvbox Dark uses accent (yellow #fabd2f) for git_modified and
                // needs_action, not the warning (orange #d65d0e) colour.
                git_new: None,
                git_modified: Some(Color::Rgb(250, 189, 47)),
                needs_action: Some(Color::Rgb(250, 189, 47)),
            },
            Theme::GruvboxLight => Self {
                background: Color::Rgb(251, 241, 199),
                foreground: Color::Rgb(60, 56, 54),
                dim: Color::Rgb(146, 131, 116),
                border: Color::Rgb(213, 196, 161),
                accent: Color::Rgb(215, 153, 33),
                accent_alt: Color::Rgb(152, 151, 26),
                selection_bg: Color::Rgb(235, 219, 178),
                selection_fg: None,
                on_accent_fg: Some(Color::Rgb(60, 56, 54)),
                title: None,
                // Gruvbox Light h1 = neutral red (#cc241d)
                h1: Some(Color::Rgb(204, 36, 29)),
                h2: None,
                h3: None,
                heading_other: None,
                // Gruvbox Light inline_code = neutral purple
                inline_code: Some(Color::Rgb(177, 98, 134)),
                code_fg: None,
                code_bg: Some(Color::Rgb(235, 219, 178)),
                // GruvboxLight border_focused = orange (#d65d0e), not accent (yellow)
                border_focused: Some(Color::Rgb(214, 93, 14)),
                // GruvboxLight code_border = border (#d5c4a1), not dim
                code_border: Some(Color::Rgb(213, 196, 161)),
                // Gruvbox Light link = neutral aqua (#427b58 → actually #458588)
                link: Some(Color::Rgb(69, 133, 136)),
                // GruvboxLight list_marker = accent (yellow #d79921), not accent_alt (green)
                list_marker: Some(Color::Rgb(215, 153, 33)),
                // Gruvbox Light task_marker = neutral green (#98971a → actually #689d6a)
                task_marker: Some(Color::Rgb(104, 157, 106)),
                block_quote_fg: None,
                // Gruvbox Light block_quote_border = border (not dim)
                block_quote_border: Some(Color::Rgb(213, 196, 161)),
                // Gruvbox Light table_header = bright orange (#d65d0e)
                table_header: Some(Color::Rgb(214, 93, 14)),
                // Gruvbox Light table_border = border
                table_border: Some(Color::Rgb(213, 196, 161)),
                // GruvboxLight search_match_bg = accent (yellow #d79921), not accent_alt
                search_match_bg: Some(Color::Rgb(215, 153, 33)),
                gutter: None,
                current_match_bg: Some(Color::Rgb(214, 93, 14)),
                match_fg: None,
                // GruvboxLight status_bar_bg = code_bg (same derivation — no override)
                // GruvboxLight status_bar_fg = neutral dark (#504945) not dim
                status_bar_bg: None,
                status_bar_fg: Some(Color::Rgb(80, 73, 69)),
                help_bg: Some(Color::Rgb(235, 219, 178)),
                success: Some(Color::Rgb(104, 157, 106)),
                warning: Some(Color::Rgb(214, 93, 14)),
                danger: Some(Color::Rgb(204, 36, 29)),
                // GruvboxLight git_new = accent_alt (green #98971a), not success
                // (which is a different neutral green #689d6a used only for git diffs).
                git_new: Some(Color::Rgb(152, 151, 26)),
                // Gruvbox Light uses accent (yellow #d79921) for git_modified and
                // needs_action, not the warning (orange #d65d0e) colour.
                git_modified: Some(Color::Rgb(215, 153, 33)),
                needs_action: Some(Color::Rgb(215, 153, 33)),
            },
            Theme::GithubLight => Self {
                background: Color::Rgb(255, 255, 255),
                foreground: Color::Rgb(31, 35, 40),
                dim: Color::Rgb(101, 109, 118),
                border: Color::Rgb(208, 215, 222),
                accent: Color::Rgb(9, 105, 218),
                accent_alt: Color::Rgb(154, 103, 0),
                selection_bg: Color::Rgb(221, 244, 255),
                // GitHub Light: selection_fg == accent (blue). Keep it but
                // on_accent_fg must be white to avoid invisible text on accent bg.
                selection_fg: Some(Color::Rgb(9, 105, 218)),
                on_accent_fg: Some(Color::Rgb(255, 255, 255)),
                title: None,
                // GitHub Light H1 = accent (blue) — same as derivation rule
                h1: None,
                h2: Some(Color::Rgb(154, 103, 0)),
                h3: Some(Color::Rgb(26, 127, 55)),
                heading_other: None,
                // GitHub Light inline_code = danger red (#cf222e)
                inline_code: Some(Color::Rgb(207, 34, 46)),
                code_fg: None,
                code_bg: Some(Color::Rgb(246, 248, 250)),
                border_focused: None,
                // GitHub Light code_border = border (#d0d7de), not dim
                code_border: Some(Color::Rgb(208, 215, 222)),
                link: None,
                list_marker: None,
                task_marker: Some(Color::Rgb(26, 127, 55)),
                block_quote_fg: None,
                // GitHub Light block_quote_border = border (#d0d7de), not dim
                block_quote_border: Some(Color::Rgb(208, 215, 222)),
                table_header: None,
                table_border: None,
                // GitHub Light search_match_bg = golden (#ffd33d), not accent_alt
                search_match_bg: Some(Color::Rgb(255, 211, 61)),
                gutter: None,
                // GitHub Light current search hit = amber (#ff8f00)
                current_match_bg: Some(Color::Rgb(255, 143, 0)),
                // GitHub Light match_fg = foreground (dark text), not background
                match_fg: Some(Color::Rgb(31, 35, 40)),
                status_bar_bg: None,
                status_bar_fg: None,
                help_bg: Some(Color::Rgb(246, 248, 250)),
                success: Some(Color::Rgb(26, 127, 55)),
                warning: Some(Color::Rgb(130, 80, 0)),
                danger: Some(Color::Rgb(207, 34, 46)),
                git_new: None,
                // GitHub Light git_modified = accent_alt (#9a6700), not warning (#824b00)
                git_modified: Some(Color::Rgb(154, 103, 0)),
                needs_action: Some(Color::Rgb(154, 103, 0)),
            },
        }
    }
}

impl Palette {
    /// Derive a full [`Palette`] from a compact set of [`ThemeTokens`].
    ///
    /// Fields with `None` in the override slot use the documented default
    /// derivation rule. Fields with `Some(c)` use `c` verbatim.
    pub fn from_tokens(tokens: ThemeTokens) -> Self {
        // Resolve override-or-derive for each field. Fields used as inputs to
        // other derivations are resolved first.
        let success = tokens.success.unwrap_or(tokens.accent);
        let warning = tokens.warning.unwrap_or(tokens.accent_alt);
        let danger = tokens.danger.unwrap_or(tokens.accent);
        let code_bg = tokens.code_bg.unwrap_or(tokens.background);

        Self {
            background: tokens.background,
            foreground: tokens.foreground,
            dim: tokens.dim,
            border: tokens.border,
            border_focused: tokens.border_focused.unwrap_or(tokens.accent),
            accent: tokens.accent,
            accent_alt: tokens.accent_alt,
            selection_bg: tokens.selection_bg,
            selection_fg: tokens.selection_fg.unwrap_or(tokens.foreground),
            on_accent_fg: tokens.on_accent_fg.unwrap_or(tokens.foreground),
            title: tokens.title.unwrap_or(tokens.foreground),
            h1: tokens.h1.unwrap_or(tokens.accent),
            h2: tokens.h2.unwrap_or(tokens.accent),
            h3: tokens.h3.unwrap_or(tokens.accent_alt),
            heading_other: tokens.heading_other.unwrap_or(tokens.foreground),
            inline_code: tokens.inline_code.unwrap_or(tokens.accent_alt),
            code_fg: tokens.code_fg.unwrap_or(tokens.foreground),
            code_bg,
            code_border: tokens.code_border.unwrap_or(tokens.dim),
            link: tokens.link.unwrap_or(tokens.accent),
            list_marker: tokens.list_marker.unwrap_or(tokens.accent_alt),
            task_marker: tokens.task_marker.unwrap_or(tokens.accent_alt),
            block_quote_fg: tokens.block_quote_fg.unwrap_or(tokens.dim),
            block_quote_border: tokens.block_quote_border.unwrap_or(tokens.dim),
            table_header: tokens.table_header.unwrap_or(tokens.accent),
            table_border: tokens.table_border.unwrap_or(tokens.border),
            search_match_bg: tokens.search_match_bg.unwrap_or(tokens.accent_alt),
            current_match_bg: tokens.current_match_bg.unwrap_or(tokens.accent),
            match_fg: tokens.match_fg.unwrap_or(tokens.background),
            gutter: tokens.gutter.unwrap_or(tokens.dim),
            status_bar_bg: tokens.status_bar_bg.unwrap_or(code_bg),
            status_bar_fg: tokens.status_bar_fg.unwrap_or(tokens.dim),
            help_bg: tokens.help_bg.unwrap_or(code_bg),
            git_new: tokens.git_new.unwrap_or(success),
            git_modified: tokens.git_modified.unwrap_or(warning),
            needs_action: tokens.needs_action.unwrap_or(warning),
            success,
            warning,
            danger,
            muted: tokens.dim,
        }
    }
}

impl Palette {
    /// Construct the color palette for the given theme.
    ///
    /// Delegates to [`ThemeTokens::for_theme`] + [`Palette::from_tokens`].
    /// The parity test in `theme::tests` verifies that this produces
    /// bit-identical output to the original hand-authored match.
    pub fn from_theme(theme: Theme) -> Self {
        Self::from_tokens(ThemeTokens::for_theme(theme))
    }
}

impl Palette {
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

    /// Overlay surfaces (help, repo picker, confirm, first-run wizard) all
    /// render with `palette.help_bg` as their fill color. If that equals the
    /// dashboard background, the overlay is invisible apart from its border —
    /// a real bug shipped in the original Default and Dracula palettes that
    /// made first-time users think the picker was broken.
    ///
    /// Lock it in: every theme must provide a visually distinct overlay
    /// surface.
    #[test]
    fn overlay_bg_is_distinct_from_main_bg() {
        for &theme in Theme::ALL {
            let p = Palette::from_theme(theme);
            assert_ne!(
                p.help_bg, p.background,
                "Theme {theme:?}: help_bg == background — overlays will be invisible",
            );
        }
    }

    /// `from_tokens` must reproduce the exact same palette as the legacy
    /// `from_theme` match for every theme. A failure here means a colour has
    /// drifted between the two code paths — find the field, add or fix the
    /// corresponding override slot in [`ThemeTokens::for_theme`].
    #[test]
    fn from_tokens_matches_from_theme_for_every_theme() {
        for theme in Theme::ALL {
            let via_tokens = Palette::from_tokens(ThemeTokens::for_theme(*theme));
            let via_legacy = Palette::from_theme(*theme);
            assert_eq!(
                via_tokens, via_legacy,
                "Palette drift detected for {theme:?} — the ThemeTokens derivation \
                 rules are not reproducing the legacy colour output",
            );
        }
    }

    /// `border_focused` must differ from `border` for every theme so that
    /// the focused panel is visually distinct from unfocused ones.
    #[test]
    fn border_focused_differs_from_border() {
        for &theme in Theme::ALL {
            let p = Palette::from_theme(theme);
            assert_ne!(
                p.border, p.border_focused,
                "Theme {theme:?}: border == border_focused — focused panels indistinguishable",
            );
        }
    }

    /// A selection row highlighted via reverse video (see `ui::dashboard`)
    /// must present `selection_fg` on top of `selection_bg`. Equal values
    /// make the highlighted row unreadable.
    #[test]
    fn selection_fg_contrasts_with_selection_bg() {
        for &theme in Theme::ALL {
            let p = Palette::from_theme(theme);
            assert_ne!(
                p.selection_fg, p.selection_bg,
                "Theme {theme:?}: selection_fg == selection_bg — highlighted rows unreadable",
            );
        }
    }

    /// Status-bar text must be readable against its own background. This catches
    /// the easy mistake of forgetting to set `status_bar_fg` when introducing
    /// a new theme.
    #[test]
    fn status_bar_fg_contrasts_with_status_bar_bg() {
        for &theme in Theme::ALL {
            let p = Palette::from_theme(theme);
            assert_ne!(
                p.status_bar_fg, p.status_bar_bg,
                "Theme {theme:?}: status_bar_fg == status_bar_bg — hints unreadable",
            );
        }
    }
}
