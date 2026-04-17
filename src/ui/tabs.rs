//! Ordered collection of open repository tabs with an active-tab pointer.
// `close`, `activate_previous`, and `needs_action_count` are used in later phases.
#![allow(dead_code)]
//!
//! Each tab represents one `owner/name` repository slug. The tab bar shows
//! the repo name and (in Phase 3) a badge with the count of items needing
//! attention.

/// Maximum number of tabs that can be open simultaneously.
pub const MAX_TABS: usize = 32;

/// Opaque stable identifier for a tab.
///
/// Uses a monotonically increasing counter so the id is stable across
/// insertions and removals (unlike a bare index, which shifts when tabs close).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabId(pub u32);

/// All state owned by a single repository tab.
#[derive(Debug, Clone)]
pub struct Tab {
    /// Stable identifier.
    pub id: TabId,
    /// `owner/name` repository slug, e.g. `"rust-lang/rust"`.
    pub repo: String,
    /// Count of items that need attention (PRs / issues). `None` until the
    /// first fetch completes in Phase 3.
    pub needs_action_count: Option<usize>,
}

/// The outcome of a [`Tabs::open_or_focus`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOutcome {
    /// An existing tab for this repo was focused (deduplicated).
    Focused,
    /// A new tab was pushed and activated.
    Opened,
    /// The tab cap (`MAX_TABS`) was reached; nothing changed.
    Capped,
}

/// Ordered collection of open repository tabs with an active-tab pointer.
#[allow(clippy::struct_field_names)]
pub struct Tabs {
    pub tabs: Vec<Tab>,
    /// The currently visible tab (by id).
    pub active: Option<TabId>,
    /// The previously active tab, used for backtick-style navigation.
    pub previous: Option<TabId>,
    /// Monotonically increasing id counter.
    next_id: u32,
}

impl Tabs {
    /// Create an empty tab set with no active tab.
    pub fn new() -> Self {
        Self { tabs: Vec::new(), active: None, previous: None, next_id: 0 }
    }

    fn alloc_id(&mut self) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        id
    }

    fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// Return a shared reference to the active tab, if any.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tabs.get(self.index_of(id)?))
    }

    /// Return the 0-based index of the active tab in the `tabs` slice.
    pub fn active_index(&self) -> Option<usize> {
        self.active.and_then(|id| self.index_of(id))
    }

    /// Make `id` the active tab, recording the previous active for backtick navigation.
    pub fn set_active(&mut self, id: TabId) {
        if self.active != Some(id) {
            self.previous = self.active;
            self.active = Some(id);
        }
    }

    /// Open or focus a tab for the given repo slug.
    ///
    /// - If the repo is already open, activate that tab and return `Focused`.
    /// - If `tabs.len() >= MAX_TABS`, refuse and return `Capped`.
    /// - Otherwise push a new tab, activate it, and return `Opened`.
    pub fn open_or_focus(&mut self, repo: &str) -> (TabId, OpenOutcome) {
        // Deduplicate: if already open, just switch.
        if let Some(existing) = self.tabs.iter().find(|t| t.repo == repo) {
            let id = existing.id;
            self.set_active(id);
            return (id, OpenOutcome::Focused);
        }

        // Enforce the tab cap.
        if self.tabs.len() >= MAX_TABS {
            let fallback = self.active.unwrap_or(TabId(0));
            return (fallback, OpenOutcome::Capped);
        }

        let id = self.alloc_id();
        self.tabs.push(Tab { id, repo: repo.to_owned(), needs_action_count: None });
        self.set_active(id);
        (id, OpenOutcome::Opened)
    }

    /// Close the tab with `id`. Returns `true` if the tab was found and removed.
    ///
    /// After closing, the active tab is updated: the previously active tab if
    /// it still exists, otherwise the neighbour at the same index (clamped).
    pub fn close(&mut self, id: TabId) -> bool {
        let Some(idx) = self.index_of(id) else {
            return false;
        };
        self.tabs.remove(idx);

        if self.tabs.is_empty() {
            self.active = None;
            self.previous = None;
            return true;
        }

        if let Some(prev) = self.previous
            && prev != id
            && self.index_of(prev).is_some()
        {
            self.previous = None;
            self.active = Some(prev);
        } else {
            let new_idx = idx.min(self.tabs.len() - 1);
            self.active = Some(self.tabs[new_idx].id);
            self.previous = None;
        }
        true
    }

    /// Return the number of open tabs.
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// Return `true` when no tabs are open.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Activate the tab after the current one, wrapping around.
    /// No-op when fewer than 2 tabs are open.
    pub fn next(&mut self) {
        let Some(idx) = self.active_index() else {
            return;
        };
        if self.tabs.is_empty() {
            return;
        }
        let next_idx = (idx + 1) % self.tabs.len();
        let id = self.tabs[next_idx].id;
        self.set_active(id);
    }

    /// Activate the tab before the current one, wrapping around.
    /// No-op when fewer than 2 tabs are open.
    pub fn prev(&mut self) {
        let Some(idx) = self.active_index() else {
            return;
        };
        if self.tabs.is_empty() {
            return;
        }
        let prev_idx = if idx == 0 { self.tabs.len() - 1 } else { idx - 1 };
        let id = self.tabs[prev_idx].id;
        self.set_active(id);
    }

    /// Activate a tab by 0-based index. Out-of-range is a silent no-op.
    pub fn set_active_by_index(&mut self, idx: usize) {
        if let Some(tab) = self.tabs.get(idx) {
            let id = tab.id;
            self.set_active(id);
        }
    }

    /// Activate the previously active tab (backtick navigation).
    pub fn activate_previous(&mut self) {
        let Some(prev) = self.previous else {
            return;
        };
        if self.index_of(prev).is_none() {
            self.previous = None;
            return;
        }
        let current = self.active;
        self.active = Some(prev);
        self.previous = current;
    }
}

impl Default for Tabs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_or_focus_creates_new_tab() {
        let mut tabs = Tabs::new();
        let (_, outcome) = tabs.open_or_focus("rust-lang/rust");
        assert_eq!(outcome, OpenOutcome::Opened);
        assert_eq!(tabs.len(), 1);
        assert!(tabs.active.is_some());
    }

    #[test]
    fn open_or_focus_dedupes_by_repo() {
        let mut tabs = Tabs::new();
        tabs.open_or_focus("rust-lang/rust");
        let (_, outcome) = tabs.open_or_focus("rust-lang/rust");
        assert_eq!(outcome, OpenOutcome::Focused);
        assert_eq!(tabs.len(), 1);
    }

    #[test]
    fn open_or_focus_caps_at_max_tabs() {
        let mut tabs = Tabs::new();
        for i in 0..MAX_TABS {
            tabs.open_or_focus(&format!("owner/repo{i}"));
        }
        assert_eq!(tabs.len(), MAX_TABS);
        let (_, outcome) = tabs.open_or_focus("overflow/repo");
        assert_eq!(outcome, OpenOutcome::Capped);
        assert_eq!(tabs.len(), MAX_TABS);
    }

    #[test]
    fn close_active_last_tab() {
        let mut tabs = Tabs::new();
        let (id, _) = tabs.open_or_focus("a/b");
        assert!(tabs.close(id));
        assert_eq!(tabs.len(), 0);
        assert!(tabs.active.is_none());
    }

    #[test]
    fn next_prev_wraparound() {
        let mut tabs = Tabs::new();
        let (a_id, _) = tabs.open_or_focus("a/a");
        tabs.open_or_focus("b/b");
        let (c_id, _) = tabs.open_or_focus("c/c");
        // Active is C (index 2), next wraps to A (index 0).
        tabs.next();
        assert_eq!(tabs.active, Some(a_id));
        // Active is A (index 0), prev wraps to C (index 2).
        tabs.prev();
        assert_eq!(tabs.active, Some(c_id));
    }

    #[test]
    fn set_active_by_index_out_of_range_is_noop() {
        let mut tabs = Tabs::new();
        tabs.open_or_focus("a/a");
        let active_before = tabs.active;
        // 0 is valid, 99 is out of range.
        tabs.set_active_by_index(99);
        assert_eq!(tabs.active, active_before);
    }

    #[test]
    fn next_noop_when_one_tab() {
        let mut tabs = Tabs::new();
        let (id, _) = tabs.open_or_focus("a/a");
        tabs.next();
        // Still on the same tab.
        assert_eq!(tabs.active, Some(id));
    }
}
