//! Application state and main event loop.
//!
//! [`App`] owns all runtime state. [`App::run`] is the async entry point that
//! draws frames and processes actions until the user quits.
//!
//! # Module layout
//!
//! | Submodule          | Contents                                             |
//! |--------------------|------------------------------------------------------|
//! | `types`            | Pure data types with no `App` dependency             |
//! | `state`            | `App` struct, constructor, and field accessors       |
//! | `actions`          | `Action` enum (public — referenced externally)       |
//! | `fetch`            | Background fetch helpers and SWR cache logic         |
//! | `action_handlers`  | `handle_action` dispatcher and confirmation helpers  |
//! | `keymap`           | Per-focus keyboard handlers                          |
//! | `mouse`            | Mouse event routing                                  |
//! | `run`              | `App::run` async entry point and auto-refresh timer  |

mod action_handlers;
pub mod actions;
mod fetch;
mod keymap;
mod mouse;
mod run;
mod state;
mod types;

#[cfg(test)]
mod tests;

// Re-export the public API so that external callers can use `crate::app::App`,
// `crate::app::Focus`, etc. without knowing the internal module layout.
pub use state::App;
// Re-export types referenced externally as `crate::app::X`.
// `FirstRunSuggestion` is only used in test code in `ui::first_run`, so the
// compiler warns about it in non-test builds — the allow silences that.
#[allow(unused_imports)]
pub use types::{FirstRunSuggestion, Focus, RepoPickerMode};
