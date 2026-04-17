//! User configuration loaded from `~/.config/octopeek/config.toml`.
// `Config::save` is used in the settings panel (Phase 4).
#![allow(dead_code)]
//!
//! All fields use `#[serde(default)]` so that older config files missing
//! newer fields still parse without error.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::Theme;

const APP_NAME: &str = "octopeek";
const CONFIG_FILE: &str = "config.toml";

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

    /// Persist settings to disk. Silently swallows any I/O error.
    pub fn save(&self) {
        let Some(path) = config_path() else {
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
}

/// Resolve the XDG config path for the octopeek config file.
fn config_path() -> Option<PathBuf> {
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
