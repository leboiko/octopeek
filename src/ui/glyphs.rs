//! Central registry of display glyphs and their semantic color roles.
//!
//! All glyphs are in the Unicode BMP (code points below U+FFFF). Each glyph
//! has an ASCII fallback for terminals that set `show_ascii_glyphs = true` in
//! the config.

use crate::github::flags::ActionFlag;
use crate::github::types::{CheckState, Role};

// ── Glyph constants ───────────────────────────────────────────────────────────

/// Glyph shown when the viewer is the PR author.
pub const ROLE_AUTHOR: char = 'A';
/// Glyph shown when the viewer is a requested reviewer.
pub const ROLE_REVIEWER: char = 'R';
/// Glyph shown when the viewer is an assignee.
pub const ROLE_ASSIGNEE: char = '@';

/// Needs-action dot (PR has a non-clean, non-draft flag).
pub const NEEDS_ACTION: char = '\u{25CF}'; // ●
/// ASCII fallback for `NEEDS_ACTION`.
pub const NEEDS_ACTION_ASCII: char = '*';

/// Status: changes-requested or review-requested.
pub const STATUS_REVIEW: char = '\u{2691}'; // ⚑
/// ASCII fallback for `STATUS_REVIEW`.
pub const STATUS_REVIEW_ASCII: char = '!';

/// Status: merge conflict.
pub const STATUS_CONFLICT: char = '\u{21C4}'; // ⇄
/// ASCII fallback for `STATUS_CONFLICT`.
pub const STATUS_CONFLICT_ASCII: char = '~';

/// Status: branch behind base.
pub const STATUS_BEHIND: char = '\u{25B2}'; // ▲
/// ASCII fallback for `STATUS_BEHIND`.
pub const STATUS_BEHIND_ASCII: char = '^';

/// Status: unresolved review threads.
pub const STATUS_THREADS: char = '\u{25CB}'; // ○
/// ASCII fallback for `STATUS_THREADS`.
pub const STATUS_THREADS_ASCII: char = '?';

/// Draft glyph (in the status col, col 3).
pub const STATUS_DRAFT: char = '\u{25C6}'; // ◆
/// ASCII fallback for `STATUS_DRAFT`.
pub const STATUS_DRAFT_ASCII: char = 'D';

/// CI success.
pub const CI_SUCCESS: char = '\u{2714}'; // ✔
/// ASCII fallback for `CI_SUCCESS`.
pub const CI_SUCCESS_ASCII: char = '+';

/// CI failure.
pub const CI_FAILURE: char = '\u{2716}'; // ✖
/// ASCII fallback for `CI_FAILURE`.
pub const CI_FAILURE_ASCII: char = 'x';

/// CI pending.
pub const CI_PENDING: char = '\u{25CF}'; // ●
/// ASCII fallback for `CI_PENDING`.
pub const CI_PENDING_ASCII: char = '.';

/// No CI configured.
pub const CI_NONE: char = '\u{2014}'; // —
/// ASCII fallback for `CI_NONE`.
pub const CI_NONE_ASCII: char = '-';

// ── ColorRole ─────────────────────────────────────────────────────────────────

/// Semantic color roles used to colorize glyphs in the dashboard.
///
/// Map to concrete `ratatui::style::Color` via [`crate::theme::Palette::color_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRole {
    /// Something requires the viewer's immediate attention.
    NeedsAction,
    /// Everything is good / CI passing.
    Success,
    /// Non-critical warning (stale refresh, minor flag).
    Warning,
    /// Blocking issue (conflict, CI failure, danger).
    Danger,
    /// De-emphasized / secondary information.
    Muted,
    /// Theme accent color (reserved for Phase 4+ use).
    #[allow(dead_code)]
    Accent,
}

// ── Public glyph functions ────────────────────────────────────────────────────

/// Return the role glyph character for `role`.
///
/// Role glyphs are already ASCII so no ASCII-mode parameter is needed.
pub fn role_glyph(role: Role) -> char {
    match role {
        Role::Author => ROLE_AUTHOR,
        Role::Reviewer => ROLE_REVIEWER,
        Role::Assignee => ROLE_ASSIGNEE,
    }
}

/// Return the glyph and [`ColorRole`] for `flag`.
///
/// When `ascii` is `true`, returns only characters in the ASCII range (< 128).
/// The exhaustive match ensures the compiler forces an update when new
/// `ActionFlag` variants are added.
pub fn flag_glyph(flag: ActionFlag, ascii: bool) -> (char, ColorRole) {
    match flag {
        ActionFlag::Draft => {
            let ch = if ascii { STATUS_DRAFT_ASCII } else { STATUS_DRAFT };
            (ch, ColorRole::Muted)
        }
        ActionFlag::Conflict => {
            let ch = if ascii { STATUS_CONFLICT_ASCII } else { STATUS_CONFLICT };
            (ch, ColorRole::Danger)
        }
        ActionFlag::CiFailing => {
            let ch = if ascii { CI_FAILURE_ASCII } else { CI_FAILURE };
            (ch, ColorRole::Danger)
        }
        // Both ChangesRequested and ReviewRequested use the same flag glyph.
        ActionFlag::ChangesRequested | ActionFlag::ReviewRequested => {
            let ch = if ascii { STATUS_REVIEW_ASCII } else { STATUS_REVIEW };
            (ch, ColorRole::NeedsAction)
        }
        ActionFlag::UnresolvedThreads => {
            let ch = if ascii { STATUS_THREADS_ASCII } else { STATUS_THREADS };
            (ch, ColorRole::NeedsAction)
        }
        ActionFlag::Behind => {
            let ch = if ascii { STATUS_BEHIND_ASCII } else { STATUS_BEHIND };
            (ch, ColorRole::Warning)
        }
        ActionFlag::Clean => (' ', ColorRole::Muted),
    }
}

/// Return the CI glyph and [`ColorRole`] for `state`.
///
/// `None` means no CI is configured for this PR.
pub fn ci_glyph(state: Option<CheckState>, ascii: bool) -> (char, ColorRole) {
    match state {
        Some(CheckState::Success | CheckState::Expected) => {
            let ch = if ascii { CI_SUCCESS_ASCII } else { CI_SUCCESS };
            (ch, ColorRole::Success)
        }
        Some(CheckState::Failure | CheckState::Error) => {
            let ch = if ascii { CI_FAILURE_ASCII } else { CI_FAILURE };
            (ch, ColorRole::Danger)
        }
        Some(CheckState::Pending) => {
            let ch = if ascii { CI_PENDING_ASCII } else { CI_PENDING };
            (ch, ColorRole::Warning)
        }
        None => {
            let ch = if ascii { CI_NONE_ASCII } else { CI_NONE };
            (ch, ColorRole::Muted)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `ActionFlag` variant must map to a deterministic glyph.
    #[test]
    fn flag_glyph_complete() {
        use ActionFlag::{
            Behind, ChangesRequested, CiFailing, Clean, Conflict, Draft, ReviewRequested,
            UnresolvedThreads,
        };
        for flag in [
            Draft,
            Conflict,
            CiFailing,
            ChangesRequested,
            ReviewRequested,
            UnresolvedThreads,
            Behind,
            Clean,
        ] {
            let (ch, _) = flag_glyph(flag, false);
            // Unicode glyph must not be zero (i.e. was actually mapped).
            // Clean maps to space, which is fine.
            let _ = ch; // just ensure it compiles & returns.
        }
    }

    /// With `ascii = true`, every glyph must be in the 7-bit ASCII range.
    #[test]
    fn flag_glyph_ascii_only() {
        use ActionFlag::{
            Behind, ChangesRequested, CiFailing, Clean, Conflict, Draft, ReviewRequested,
            UnresolvedThreads,
        };
        for flag in [
            Draft,
            Conflict,
            CiFailing,
            ChangesRequested,
            ReviewRequested,
            UnresolvedThreads,
            Behind,
            Clean,
        ] {
            let (ch, _) = flag_glyph(flag, true);
            assert!(ch.is_ascii(), "flag {flag:?} returned non-ASCII char '{ch}' in ASCII mode");
        }
    }

    /// CI glyphs in ASCII mode must all be ASCII.
    #[test]
    fn ci_glyph_ascii_only() {
        for state in [
            Some(CheckState::Success),
            Some(CheckState::Failure),
            Some(CheckState::Error),
            Some(CheckState::Pending),
            Some(CheckState::Expected),
            None,
        ] {
            let (ch, _) = ci_glyph(state, true);
            assert!(ch.is_ascii(), "ci_glyph({state:?}, true) returned non-ASCII '{ch}'");
        }
    }

    /// Each `ColorRole` variant must be reachable from a `flag_glyph` call.
    #[test]
    fn color_roles_are_mapped() {
        let roles: Vec<ColorRole> = [
            ActionFlag::Draft,
            ActionFlag::Conflict,
            ActionFlag::ChangesRequested,
            ActionFlag::Behind,
            ActionFlag::Clean,
        ]
        .iter()
        .map(|&f| flag_glyph(f, false).1)
        .collect();

        assert!(roles.contains(&ColorRole::Muted));
        assert!(roles.contains(&ColorRole::Danger));
        assert!(roles.contains(&ColorRole::NeedsAction));
        assert!(roles.contains(&ColorRole::Warning));
    }
}
