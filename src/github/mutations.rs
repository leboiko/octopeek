//! Typed GitHub mutation inputs and outputs used by the app layer.
//!
//! The HTTP client owns the wire format, but these small domain types keep the
//! app from passing stringly-typed merge methods and mutation results around.

use serde::{Deserialize, Serialize};

/// Pull request merge strategy supported by GitHub's REST merge endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeMethod {
    /// Create a merge commit.
    Merge,
    /// Squash all commits into one commit on the base branch.
    Squash,
}

impl MergeMethod {
    /// Human-readable label used in confirmations and flash messages.
    pub fn label(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash merge",
        }
    }

    /// REST API value for `merge_method`.
    pub(crate) fn rest_value(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
        }
    }
}

/// Successful pull request merge response from GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeOutcome {
    /// SHA created or advanced by the merge operation.
    pub sha: String,
    /// Human-readable message returned by GitHub.
    pub message: String,
}
