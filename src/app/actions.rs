//! All dispatchable actions in the octopeek event loop.
//!
//! `RawKey` events are produced by the input thread and translated into more
//! specific variants by focused-widget key handlers. Many variants will only
//! become active in later phases; they are defined here so the type system
//! stays authoritative throughout the codebase.

use crossterm::event::{KeyEvent, MouseEvent};

use crate::github;

/// All actions that can be dispatched through the application event loop.
///
/// Many variants are matched in `handle_action` but not yet constructed
/// anywhere — those belong to Phase 2 onward. The enum-level attribute keeps
/// compilation clean without masking missing variants at call sites.
#[allow(dead_code)]
#[derive(Debug)]
pub enum Action {
    // ── Lifecycle ─────────────────────────────────────────────────────────────
    /// Exit the application cleanly.
    Quit,

    // ── Raw input ─────────────────────────────────────────────────────────────
    /// Raw terminal key event — mapped to a concrete action based on focus.
    RawKey(KeyEvent),

    /// Terminal was resized to (width, height).
    Resize(u16, u16),

    /// Raw mouse event forwarded from crossterm.
    Mouse(MouseEvent),

    // ── Tab management ────────────────────────────────────────────────────────
    /// Switch to tab at the given 0-based index.
    SwitchTab(usize),

    /// Activate the next tab (wraps around).
    NextTab,

    /// Activate the previous tab (wraps around).
    PrevTab,

    // ── Data refresh ──────────────────────────────────────────────────────────
    /// Refresh the currently active repo tab.
    /// Phase 2: triggers a GitHub GraphQL fetch for the active repo.
    Refresh,

    /// Refresh all repo tabs.
    /// Phase 2: triggers GitHub GraphQL fetches for every configured repo.
    RefreshAll,

    // ── Navigation within a tab ───────────────────────────────────────────────
    /// Open the PR/issue detail view for the currently selected item.
    /// Phase 3: populates the detail panel from cached data.
    OpenDetail,

    /// Return from the detail view to the dashboard list.
    /// Phase 3: restores the dashboard focus.
    BackToDashboard,

    /// Toggle the active tab between PR view and Issue view.
    /// Phase 3: flips `ViewMode` and re-renders the list.
    ToggleView,

    // ── PR / issue actions ────────────────────────────────────────────────────
    /// Open the selected PR or issue in the system browser.
    /// Phase 4: calls `open::that(url)`.
    OpenInBrowser,

    /// Copy the URL of the selected item to the clipboard.
    /// Phase 4: uses the `arboard` crate.
    CopyUrl,

    /// Begin a branch checkout flow for the selected PR.
    /// Phase 5: shows a confirmation overlay before running `git checkout`.
    CheckoutBranch,

    /// Confirm or cancel the branch checkout initiated by `CheckoutBranch`.
    /// `true` = confirmed, `false` = cancelled.
    /// Phase 5: runs `git checkout <branch>` in the current working directory.
    ConfirmCheckout(bool),

    // ── Overlays ──────────────────────────────────────────────────────────────
    /// Open the repo-picker overlay so the user can add or remove repos.
    /// Phase 3: renders the picker widget.
    OpenRepoPicker,

    /// Toggle the help overlay.
    OpenHelp,

    // ── GitHub data ───────────────────────────────────────────────────────────
    /// A background fetch has been kicked off; `fetching` is now `true`.
    InboxFetchStarted,

    /// The GitHub inbox was successfully fetched and is ready to display.
    InboxLoaded(Box<github::Inbox>),

    /// A GitHub inbox fetch failed; the string is a human-readable description.
    FetchFailed(String),

    // ── Detail fetching ───────────────────────────────────────────────────────
    /// Request a background fetch of full PR detail.
    ///
    /// Fields: `(repo_slug, pr_number)`.
    FetchPrDetail(String, u32),

    /// Request a background fetch of full issue detail.
    ///
    /// Fields: `(repo_slug, issue_number)`.
    FetchIssueDetail(String, u32),

    /// Full PR detail was successfully fetched and is ready to display.
    PrDetailLoaded(Box<github::detail::PrDetail>),

    /// Full issue detail was successfully fetched and is ready to display.
    IssueDetailLoaded(Box<github::detail::IssueDetail>),

    /// A detail fetch failed; the string is a human-readable description.
    DetailFetchFailed(String),
}
