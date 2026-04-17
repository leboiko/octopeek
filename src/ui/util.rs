//! Shared UI utilities used across multiple rendering modules.

use chrono::{DateTime, Utc};

/// Format `dt` as a human-readable delta from now: "14s ago", "3m ago", "1h ago", etc.
///
/// # Arguments
///
/// * `dt` - The timestamp to humanize relative to `Utc::now()`.
///
/// # Returns
///
/// A short human-readable string representing the elapsed time.
pub fn humanize_delta(dt: &DateTime<Utc>) -> String {
    // `.max(0)` ensures non-negative before casting; `cast_unsigned` is not
    // available on stable Rust 1.88. The `.max(0)` guard makes the sign loss safe.
    #[allow(clippy::cast_sign_loss)]
    let secs = (Utc::now() - *dt).num_seconds().max(0) as u64;
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Truncate `s` to at most `max_chars` Unicode characters, appending `…` if truncated.
///
/// # Arguments
///
/// * `s`         - The string to truncate.
/// * `max_chars` - Maximum number of Unicode scalar values in the result.
pub fn truncate(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('\u{2026}'); // …
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_unchanged() {
        assert_eq!(truncate("hello", 20), "hello");
    }

    #[test]
    fn truncate_at_limit_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_appends_ellipsis() {
        let t = truncate("hello world", 8);
        assert!(t.chars().count() <= 8);
        assert!(t.ends_with('\u{2026}'));
    }

    #[test]
    fn truncate_zero_max_returns_empty() {
        assert_eq!(truncate("anything", 0), "");
    }
}
