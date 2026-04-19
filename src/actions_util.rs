//! One-shot OS utility actions: opening URLs and copying text to the clipboard.
//!
//! Both functions are synchronous — they delegate to the OS immediately and
//! return. They must not be called from inside an `async` context on a
//! single-threaded executor; use `tokio::task::spawn_blocking` if needed.

// These functions are wired up by the Phase 4 detail-UI agent via
// `App::handle_action`; the dead_code lint fires because callers live in a
// separate parallel change that is not yet merged.
#![allow(dead_code)]

use anyhow::{Context as _, bail};

/// Open `url` in the system default browser.
///
/// Wraps the [`open`] crate, which dispatches to `open` on macOS,
/// `xdg-open` on Linux, and `start` on Windows.
///
/// Rejects any URL that does not begin with `https://`. GitHub's GraphQL
/// responses are the only source feeding this function today and always
/// return `https://` URLs, but refusing other schemes stops a hypothetical
/// malicious API response (or a future code path) from triggering
/// `file://`, `ssh://`, or custom-scheme handlers that would dispatch
/// native commands on macOS and Linux.
///
/// # Errors
///
/// Returns an error if the URL does not start with `https://`, or if the
/// OS command fails to launch. The returned message is suitable for display
/// in the status bar.
pub fn open_url_in_browser(url: &str) -> anyhow::Result<()> {
    if !url.starts_with("https://") {
        bail!("refusing to open non-https URL: {url}");
    }
    open::that(url).with_context(|| format!("failed to open URL in browser: {url}"))
}

/// Copy `text` to the system clipboard.
///
/// Uses [`arboard::Clipboard`] for cross-platform clipboard access.
///
/// # Errors
///
/// Returns an error if the clipboard is unavailable (e.g. headless Linux
/// without X11/Wayland, SSH without display forwarding) or if writing fails.
/// The error message is suitable for display in the status bar as a fallback.
pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    // `arboard::Clipboard::new()` fails on headless systems — the error message
    // clearly indicates the cause (e.g. "No X11 display connection").
    let mut clipboard =
        arboard::Clipboard::new().context("clipboard unavailable on this system")?;
    clipboard.set_text(text).context("failed to write text to clipboard")
}
