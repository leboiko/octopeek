//! Session state persisted to disk across launches.
//!
//! Stored at the XDG state path (`~/.local/state/octopeek/state.toml` on
//! Linux; `~/Library/Application Support/octopeek/state.toml` on macOS).

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::warn;

const APP_NAME: &str = "octopeek";
const STATE_FILE: &str = "state.toml";

thread_local! {
    /// Per-thread override for the state directory. See [`crate::config`] for
    /// the matching facility on the config side — same rationale, same shape.
    static STATE_DIR_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Install a per-thread override for the state directory.
///
/// Intended for test use.
#[allow(dead_code)] // Used by integration-style tests via `with_state_dir_override`.
pub fn set_state_dir_override(dir: impl Into<PathBuf>) {
    let dir: PathBuf = dir.into();
    STATE_DIR_OVERRIDE.with(|c| *c.borrow_mut() = Some(dir));
}

/// Run `f` with a per-thread state-directory override, then restore whatever
/// override (if any) was previously in place. Safe across panics.
#[allow(dead_code)] // Used by tests; the binary never calls this directly.
pub fn with_state_dir_override<R>(dir: impl AsRef<Path>, f: impl FnOnce() -> R) -> R {
    struct Guard(Option<PathBuf>);
    impl Drop for Guard {
        fn drop(&mut self) {
            STATE_DIR_OVERRIDE.with(|c| *c.borrow_mut() = self.0.take());
        }
    }

    let previous = STATE_DIR_OVERRIDE.with(|c| c.borrow_mut().replace(dir.as_ref().to_path_buf()));
    let _guard = Guard(previous);
    f()
}

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

/// Default sidebar width (cells) when nothing has been saved yet.
pub const DEFAULT_SIDEBAR_WIDTH: u16 = 28;

fn default_sidebar_width() -> u16 {
    DEFAULT_SIDEBAR_WIDTH
}

/// Full persisted session for the application.
///
/// Deserialized via [`SessionCompat`] for backwards-compatibility if the
/// schema evolves in a future release.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(from = "SessionCompat")]
pub struct AppSession {
    /// 0-based index of the active tab.
    pub active_tab_index: usize,
    /// Per-repo view mode, keyed by `owner/name` repo slug.
    pub per_repo_view: HashMap<String, ViewMode>,
    /// Last sidebar width (cells) in the PR detail view. Persisted so
    /// `[`/`]` adjustments survive relaunch.
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,
    /// Whether the PR detail sidebar was hidden on last exit.
    #[serde(default)]
    pub sidebar_hidden: bool,
}

impl Default for AppSession {
    fn default() -> Self {
        Self {
            active_tab_index: 0,
            per_repo_view: HashMap::new(),
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            sidebar_hidden: false,
        }
    }
}

/// Untagged union that handles the current format plus older on-disk shapes.
///
/// Serde tries variants top-to-bottom; the richest match wins. New fields
/// default when absent so state files written by older builds still load.
#[derive(Deserialize)]
#[serde(untagged)]
enum SessionCompat {
    /// Current format — everything we know how to persist.
    New {
        active_tab_index: usize,
        per_repo_view: HashMap<String, ViewMode>,
        #[serde(default = "default_sidebar_width")]
        sidebar_width: u16,
        #[serde(default)]
        sidebar_hidden: bool,
    },
    /// Legacy fallback: any unknown object shape maps to defaults.
    Legacy {},
}

impl From<SessionCompat> for AppSession {
    fn from(v: SessionCompat) -> Self {
        match v {
            SessionCompat::New {
                active_tab_index,
                per_repo_view,
                sidebar_width,
                sidebar_hidden,
            } => Self { active_tab_index, per_repo_view, sidebar_width, sidebar_hidden },
            SessionCompat::Legacy {} => Self::default(),
        }
    }
}

impl AppSession {
    /// Load session state from disk, returning defaults on any failure.
    ///
    /// A missing file is treated as first-run and silently returns defaults.
    /// A malformed file logs a `warn!` with the parse error before defaulting.
    pub fn load() -> Self {
        let Some(path) = state_path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str(&text) {
            Ok(session) => session,
            Err(e) => {
                warn!(
                    "failed to parse session at {}: {e}; falling back to defaults",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// Persist session state to disk.
    ///
    /// Logs but does not fail on I/O errors — state loss on a read-only disk
    /// is preferable to a crash at shutdown.
    pub fn save(&self) {
        let Some(path) = state_path() else {
            warn!("cannot resolve state path; skipping save");
            return;
        };
        if let Some(parent) = path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            warn!("failed to create state dir {}: {e}", parent.display());
            return;
        }
        let text = match toml::to_string_pretty(self) {
            Ok(t) => t,
            Err(e) => {
                warn!("failed to serialize state: {e}");
                return;
            }
        };
        if let Err(e) = fs::write(&path, text) {
            warn!("failed to write state to {}: {e}", path.display());
        }
    }

    /// Set the view mode for a given repo slug and persist immediately.
    #[allow(dead_code)] // Called from the Phase 3 toggle-view action.
    pub fn set_view_mode(&mut self, repo: &str, mode: ViewMode) {
        self.per_repo_view.insert(repo.to_owned(), mode);
        self.save();
    }

    /// Get the current view mode for a repo, defaulting to `Prs`.
    #[allow(dead_code)] // Called from the Phase 3 dashboard renderer.
    pub fn view_mode(&self, repo: &str) -> ViewMode {
        self.per_repo_view.get(repo).copied().unwrap_or_default()
    }
}

/// Resolve the platform path for the state file.
///
/// Honors the per-thread override installed via [`set_state_dir_override`];
/// otherwise prefers `dirs::state_dir()` (XDG `$XDG_STATE_HOME` on Linux) and
/// falls back to `dirs::data_dir()` on platforms (e.g., macOS) that have no
/// dedicated state directory.
fn state_path() -> Option<PathBuf> {
    if let Some(mut p) = STATE_DIR_OVERRIDE.with(|c| c.borrow().clone()) {
        p.push(STATE_FILE);
        return Some(p);
    }
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

    /// An older state.toml without `sidebar_width` / `sidebar_hidden` must
    /// still load, with the new fields taking their defaults.
    #[test]
    fn legacy_session_without_sidebar_fields_loads_with_defaults() {
        let toml_str = "active_tab_index = 0\n[per_repo_view]\n";
        let session: AppSession = toml::from_str(toml_str).expect("deserialize");
        assert_eq!(session.sidebar_width, DEFAULT_SIDEBAR_WIDTH);
        assert!(!session.sidebar_hidden);
    }

    /// Round-trip a session carrying non-default sidebar state.
    #[test]
    fn session_sidebar_state_round_trips() {
        let session =
            AppSession { sidebar_width: 42, sidebar_hidden: true, ..AppSession::default() };

        let serialized = toml::to_string_pretty(&session).expect("serialize");
        let restored: AppSession = toml::from_str(&serialized).expect("deserialize");

        assert_eq!(restored.sidebar_width, 42);
        assert!(restored.sidebar_hidden);
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
