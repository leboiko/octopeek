//! GitHub data layer: auth, types, GraphQL query, HTTP client, and action flags.

pub mod auth;
pub mod cache;
pub mod client;
pub mod detail;
pub mod flags;
pub mod query;
pub mod types;

/// Display string substituted for deleted/ghost GitHub accounts.
///
/// GitHub's GraphQL schema makes `author` nullable — a null value can mean
/// a deleted account, a suspended user, or (rarely) a bot whose identity
/// has been retracted. Rendering an empty string for any of these looked
/// broken in the UI; the sentinel makes the state visible to the reader.
pub(crate) const AUTHOR_DELETED: &str = "[deleted]";

/// Resolve an optional author login to a display string, substituting
/// [`AUTHOR_DELETED`] when the upstream value is `None`.
#[inline]
pub(crate) fn author_or_deleted(login: Option<String>) -> String {
    login.unwrap_or_else(|| AUTHOR_DELETED.to_owned())
}

// `Cached` is used in tests and by downstream callers; suppress the
// warning until production code starts accessing it directly.
#[allow(unused_imports)]
pub use cache::{Cached, DetailCache};
pub use client::Client;
// Phase 3 will reference ActionFlag from the UI layer; suppress the unused
// warning until then so we don't prematurely wire in a dependency.
#[allow(unused_imports)]
pub use flags::ActionFlag;
pub use types::Inbox;
