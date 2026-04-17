//! User configuration loaded from the platform config directory.
//!
//! Location:
//!
//! - **Linux:** `$XDG_CONFIG_HOME/octopeek/config.toml` (typically `~/.config/octopeek/config.toml`).
//! - **macOS:** `~/Library/Application Support/octopeek/config.toml`.
//! - **Windows:** `%APPDATA%\octopeek\config.toml`.
//!
//! All fields use `#[serde(default)]` so that older config files missing
//! newer fields still parse without error.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::theme::Theme;

const APP_NAME: &str = "octopeek";
const CONFIG_FILE: &str = "config.toml";

thread_local! {
    /// Per-thread override of the config directory.
    ///
    /// When `Some`, [`config_path`] returns `<dir>/config.toml` instead of the
    /// platform default. Tests set this to a `tempfile::TempDir` so they never
    /// touch the user's real filesystem.
    ///
    /// Thread-local (not a process global) so that `cargo test`'s default
    /// parallel test execution cannot race between tests.
    static CONFIG_DIR_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Install a per-thread override for the config directory.
///
/// Intended for test use. Clear with [`clear_config_dir_override`] when done.
#[allow(dead_code)] // Used exclusively by tests; the binary never calls this directly.
pub fn set_config_dir_override(dir: impl Into<PathBuf>) {
    let dir: PathBuf = dir.into();
    CONFIG_DIR_OVERRIDE.with(|c| *c.borrow_mut() = Some(dir));
}

/// Remove the per-thread config-directory override installed by
/// [`set_config_dir_override`].
#[allow(dead_code)] // Used exclusively by tests.
pub fn clear_config_dir_override() {
    CONFIG_DIR_OVERRIDE.with(|c| *c.borrow_mut() = None);
}

/// Run `f` with a per-thread config-directory override, then restore whatever
/// override (if any) was previously in place. Safe across panics.
///
/// Preferred over the raw setter for test code because it cannot leak state
/// between tests even when the test body panics.
#[allow(dead_code)] // Used exclusively by tests.
pub fn with_config_dir_override<R>(dir: impl AsRef<Path>, f: impl FnOnce() -> R) -> R {
    struct Guard(Option<PathBuf>);
    impl Drop for Guard {
        fn drop(&mut self) {
            CONFIG_DIR_OVERRIDE.with(|c| *c.borrow_mut() = self.0.take());
        }
    }

    let previous = CONFIG_DIR_OVERRIDE.with(|c| c.borrow_mut().replace(dir.as_ref().to_path_buf()));
    let _guard = Guard(previous);
    f()
}

/// All persisted user settings.
///
/// `#[serde(default)]` on every field ensures that config files written by
/// older versions of the app (missing newer fields) still parse correctly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Active color theme.
    #[serde(default)]
    pub theme: Theme,

    /// Repositories to show in the inbox, in `owner/name` format.
    /// Example: `["octocat/Hello-World", "rust-lang/rust"]`
    #[serde(default)]
    pub repos: Vec<String>,

    /// When `Some(n)`, a background task emits `Action::RefreshAll` every `n`
    /// seconds.  `None` (the default) disables auto-refresh — the user must
    /// press `r` or `R` manually.
    #[serde(default)]
    pub auto_refresh_seconds: Option<u32>,

    /// When `true`, use ASCII box-drawing characters instead of Unicode glyphs
    /// for borders. Useful for terminals with limited glyph support.
    #[serde(default)]
    pub show_ascii_glyphs: bool,
}

impl Config {
    /// Load settings from disk, returning defaults on any I/O or parse failure.
    ///
    /// Failures are silently swallowed: if the config file is missing or
    /// malformed the app launches with defaults rather than crashing.
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    /// Persist settings to disk.
    ///
    /// Reached only from the Phase 4 settings panel; logs but does not fail
    /// on I/O errors so a read-only config dir never brings the UI down.
    #[allow(dead_code)] // Called from the Phase 4 settings panel.
    pub fn save(&self) {
        let Some(path) = config_path() else {
            warn!("cannot resolve config path; skipping save");
            return;
        };
        if let Some(parent) = path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            warn!("failed to create config dir {}: {e}", parent.display());
            return;
        }
        let text = match toml::to_string_pretty(self) {
            Ok(t) => t,
            Err(e) => {
                warn!("failed to serialize config: {e}");
                return;
            }
        };
        if let Err(e) = fs::write(&path, text) {
            warn!("failed to write config to {}: {e}", path.display());
        }
    }
}

/// Resolve the config path for the octopeek config file.
///
/// Honors a per-thread override installed via [`set_config_dir_override`];
/// otherwise falls back to the platform config directory returned by
/// [`dirs::config_dir`].
fn config_path() -> Option<PathBuf> {
    if let Some(mut p) = CONFIG_DIR_OVERRIDE.with(|c| c.borrow().clone()) {
        p.push(CONFIG_FILE);
        return Some(p);
    }
    let mut path = dirs::config_dir()?;
    path.push(APP_NAME);
    path.push(CONFIG_FILE);
    Some(path)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// `Config::default()` must serialize and then deserialize back to an
    /// equivalent value.
    #[test]
    fn default_config_round_trips() {
        let config = Config::default();
        let serialized = toml::to_string_pretty(&config).expect("serialization failed");
        let deserialized: Config = toml::from_str(&serialized).expect("deserialization failed");
        assert_eq!(deserialized.theme, config.theme);
        assert_eq!(deserialized.repos, config.repos);
        assert_eq!(deserialized.auto_refresh_seconds, config.auto_refresh_seconds);
        assert_eq!(deserialized.show_ascii_glyphs, config.show_ascii_glyphs);
    }

    /// A TOML file missing optional fields must deserialize to the default value
    /// for each field.
    #[test]
    fn partial_config_fills_defaults() {
        let toml_str = r#"theme = "dracula""#;
        let config: Config = toml::from_str(toml_str).expect("deserialization failed");
        assert_eq!(config.theme, Theme::Dracula);
        assert!(config.repos.is_empty());
        assert_eq!(config.auto_refresh_seconds, None);
        assert!(!config.show_ascii_glyphs);
    }

    /// `auto_refresh_seconds = 30` must parse correctly as `Some(30)`.
    #[test]
    fn auto_refresh_some_parses() {
        let toml_str = "auto_refresh_seconds = 30\n";
        let config: Config = toml::from_str(toml_str).expect("deserialization failed");
        assert_eq!(config.auto_refresh_seconds, Some(30));
    }
}
