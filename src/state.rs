//! Session state persisted to disk across launches.
// `set_view_mode` and `view_mode` are used in Phase 3's toggle-view action.
#![allow(dead_code)]
//!
//! Stored at the XDG state path (`~/.local/state/octopeek/state.toml` on
//! Linux; `~/Library/Application Support/octopeek/state.toml` on macOS).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const APP_NAME: &str = "octopeek";
const STATE_FILE: &str = "state.toml";

/// Which item type a repo tab is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    /// Show the pull request list (default).
    #[default]
    Prs,
    /// Show the issue list.
    Issues,
}

/// Full persisted session for the application.
///
/// Deserialized via [`SessionCompat`] for backwards-compatibility if the
/// schema evolves in a future release.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(from = "SessionCompat")]
pub struct AppSession {
    /// 0-based index of the active tab.
    pub active_tab_index: usize,
    /// Per-repo view mode, keyed by `owner/name` repo slug.
    pub per_repo_view: HashMap<String, ViewMode>,
}

/// Untagged union that handles both the current format and any future
/// simplified format.
///
/// Serde discriminates variants by structural matching (the `active_tab_index`
/// key is authoritative for the current `New` variant).
#[derive(Deserialize)]
#[serde(untagged)]
enum SessionCompat {
    /// Current format — includes `active_tab_index` and `per_repo_view`.
    New { active_tab_index: usize, per_repo_view: HashMap<String, ViewMode> },
    /// Legacy fallback: any unknown object shape maps to defaults.
    Legacy {},
}

impl From<SessionCompat> for AppSession {
    fn from(v: SessionCompat) -> Self {
        match v {
            SessionCompat::New { active_tab_index, per_repo_view } => {
                Self { active_tab_index, per_repo_view }
            }
            SessionCompat::Legacy {} => Self::default(),
        }
    }
}

impl AppSession {
    /// Load session state from disk, returning defaults on any failure.
    pub fn load() -> Self {
        let Some(path) = state_path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    /// Persist session state to disk. Silently swallows any I/O error.
    pub fn save(&self) {
        let Some(path) = state_path() else {
            return;
        };
        if let Some(parent) = path.parent()
            && fs::create_dir_all(parent).is_err()
        {
            return;
        }
        let Ok(text) = toml::to_string_pretty(self) else {
            return;
        };
        let _ = fs::write(&path, text);
    }

    /// Set the view mode for a given repo slug and persist immediately.
    pub fn set_view_mode(&mut self, repo: &str, mode: ViewMode) {
        self.per_repo_view.insert(repo.to_owned(), mode);
        self.save();
    }

    /// Get the current view mode for a repo, defaulting to `Prs`.
    pub fn view_mode(&self, repo: &str) -> ViewMode {
        self.per_repo_view.get(repo).copied().unwrap_or_default()
    }
}

/// Resolve the platform path for the state file.
///
/// Prefers `dirs::state_dir()` (XDG `$XDG_STATE_HOME` on Linux); falls back
/// to `dirs::data_dir()` on platforms (e.g., macOS) that have no dedicated
/// state directory.
fn state_path() -> Option<PathBuf> {
    // `or_else` is used because `dirs::state_dir()` returns `None` on macOS
    // (no XDG_STATE_HOME equivalent), so we fall back to the data dir.
    let base = dirs::state_dir().or_else(dirs::data_dir)?;
    let mut path = base;
    path.push(APP_NAME);
    path.push(STATE_FILE);
    Some(path)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn default_session_round_trips() {
        let session = AppSession::default();
        let serialized = toml::to_string_pretty(&session).expect("serialize");
        let deserialized: AppSession = toml::from_str(&serialized).expect("deserialize");
        assert_eq!(deserialized, session);
    }

    #[test]
    fn view_mode_defaults_to_prs() {
        let session = AppSession::default();
        assert_eq!(session.view_mode("rust-lang/rust"), ViewMode::Prs);
    }

    #[test]
    fn set_view_mode_persists_in_memory() {
        let mut session = AppSession::default();
        session.per_repo_view.insert("octocat/Hello-World".to_owned(), ViewMode::Issues);
        assert_eq!(session.view_mode("octocat/Hello-World"), ViewMode::Issues,);
    }

    #[test]
    fn session_new_format_deserializes() {
        let toml_str = r#"
active_tab_index = 2
[per_repo_view]
"rust-lang/rust" = "issues"
"octocat/Hello-World" = "prs"
"#;
        let session: AppSession = toml::from_str(toml_str).expect("deserialize");
        assert_eq!(session.active_tab_index, 2);
        assert_eq!(session.view_mode("rust-lang/rust"), ViewMode::Issues);
        assert_eq!(session.view_mode("octocat/Hello-World"), ViewMode::Prs);
    }
}
