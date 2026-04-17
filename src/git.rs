//! Local git operations used by the branch checkout flow.
//!
//! All functions are synchronous — git operations on local repositories
//! complete in milliseconds and do not warrant async overhead.
//! Each call spawns a short-lived `std::process::Command` child process.
//!
//! # Design note
//!
//! The functions wrap `git` CLI invocations rather than linking a git library.
//! This keeps the binary small and means behaviour is identical to what the
//! user would see in their terminal. The trade-off is that `git` must be on
//! `$PATH`; callers should guard with [`repo_cwd_is_git`] before offering
//! checkout actions.
//!
//! Unit tests for the logic of [`checkout_branch`] and [`is_working_tree_clean`]
//! require an actual git repository in the working directory.  Those tests are
//! marked `#[ignore]` so CI runs (which may execute in a non-repo temp dir) do
//! not fail.  Run them manually with `cargo test -- --ignored`.
//!
//! # Future testability
//!
//! If test coverage for the git helpers becomes important, introduce a
//! `GitOps` trait with a `MockGitOps` implementation.  The functions here
//! would become the production implementation of that trait.

use std::process::Command;

use anyhow::{Context, Result, bail};

// ── Public API ────────────────────────────────────────────────────────────────

/// Return `true` when the working directory is inside a git repository.
///
/// Runs `git rev-parse --show-toplevel` and checks the exit code.
/// A non-zero exit means "not a git repo" (or git is not on `$PATH`).
pub fn repo_cwd_is_git() -> bool {
    Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Return `true` when the working tree has no uncommitted changes.
///
/// Runs `git status --porcelain`.  Empty output (zero bytes) means the tree
/// is clean.  Staged, unstaged, and untracked files all produce output.
///
/// # Errors
///
/// Returns an error when `git` is not found or the command exits non-zero
/// (which can happen when the cwd is not inside a git repository).
pub fn is_working_tree_clean() -> Result<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .context("failed to spawn `git status --porcelain`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git status failed: {stderr}");
    }

    // `stdout` is empty when the tree is clean.
    Ok(output.stdout.is_empty())
}

/// Return the name of the currently checked-out branch.
#[allow(dead_code)] // Available for future use (e.g. display in status bar)
///
/// Runs `git rev-parse --abbrev-ref HEAD`.  Returns `"HEAD"` when in
/// detached-HEAD state.
///
/// # Errors
///
/// Returns an error when the cwd is not a git repository or `git` is absent.
pub fn current_branch() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("failed to spawn `git rev-parse --abbrev-ref HEAD`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git rev-parse failed: {stderr}");
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(branch)
}

/// Check out `branch` in the current working directory.
///
/// Runs `git checkout <branch>`.  Does **not** fetch from the remote first;
/// the branch must already exist locally or as a remote-tracking branch that
/// git can resolve automatically.  If the branch does not exist and cannot be
/// resolved from a remote, git returns a non-zero exit code and the error is
/// surfaced to the caller.
///
/// # Errors
///
/// Returns an error when the checkout fails (branch not found, merge
/// conflicts, dirty working tree, etc.).  The error message includes the
/// text from `stderr` so the UI can display it verbatim.
pub fn checkout_branch(branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["checkout", branch])
        .output()
        .context(format!("failed to spawn `git checkout {branch}`"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git checkout failed: {}", stderr.trim());
    }

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// `repo_cwd_is_git` must return a bool without panicking.
    ///
    /// In a git repo (e.g. during development) this returns `true`;
    /// in a bare temp dir it returns `false`.  Either is acceptable.
    #[test]
    fn repo_cwd_is_git_does_not_panic() {
        let _ = repo_cwd_is_git();
    }

    /// `current_branch` must return `Ok` when run inside a git repo and
    /// the result must be a non-empty string.
    ///
    /// Marked `ignore` because CI may run outside a git repository.
    #[test]
    #[ignore = "requires a git repository in cwd; run manually with `cargo test -- --ignored`"]
    fn current_branch_returns_non_empty() {
        match current_branch() {
            Ok(branch) => assert!(!branch.is_empty(), "branch name should not be empty"),
            Err(e) => panic!("current_branch failed: {e}"),
        }
    }

    /// `is_working_tree_clean` returns a bool without panicking inside a repo.
    ///
    /// Marked `ignore` for the same reason as `current_branch_returns_non_empty`.
    #[test]
    #[ignore = "requires a git repository in cwd; run manually with `cargo test -- --ignored`"]
    fn is_working_tree_clean_returns_bool() {
        match is_working_tree_clean() {
            Ok(_clean) => {} // either true or false is valid
            Err(e) => panic!("is_working_tree_clean failed: {e}"),
        }
    }

    /// Attempting to checkout a non-existent branch must return an `Err`.
    ///
    /// Marked `ignore` for the same reason as above.
    #[test]
    #[ignore = "requires a git repository in cwd; run manually with `cargo test -- --ignored`"]
    fn checkout_nonexistent_branch_returns_err() {
        let result = checkout_branch("this-branch-definitely-does-not-exist-octopeek-test");
        assert!(result.is_err(), "expected Err for non-existent branch");
    }
}
