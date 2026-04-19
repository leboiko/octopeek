//! GitHub data layer: auth, types, GraphQL query, HTTP client, and action flags.

pub mod auth;
pub mod cache;
pub mod client;
pub mod detail;
pub mod flags;
pub mod query;
pub mod types;

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
