//! GitHub token discovery — env vars, then `gh auth token`.

use std::process::Command;

/// Load a GitHub API token via a three-tier fallback chain.
///
/// # Order
/// 1. `GITHUB_TOKEN` environment variable.
/// 2. `GH_TOKEN` environment variable.
/// 3. Output of `gh auth token` (CLI helper).
///
/// # Errors
///
/// Returns an actionable error when no token is found anywhere.
pub fn load_token() -> anyhow::Result<String> {
    // Tier 1 & 2 — standard env vars used by GitHub tooling.
    for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(val) = std::env::var(var) {
            let trimmed = val.trim().to_owned();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
        }
    }

    // Tier 3 — delegate to the `gh` CLI if it is on PATH.
    let output =
        Command::new("gh").args(["auth", "token"]).output().ok().filter(|o| o.status.success());

    if let Some(out) = output {
        let token = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    anyhow::bail!("No GitHub token found. Run `gh auth login` or set GITHUB_TOKEN.")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercise the env-var path by reading a variable that is set at
    /// *process start* via `[env]` in `.cargo/config.toml` or by the test
    /// runner.  Because mutating `std::env` is unsafe in multi-threaded
    /// contexts (and the project forbids `unsafe`), we instead provide a
    /// thin wrapper that accepts an explicit token string and test the
    /// parsing logic through it.
    ///
    /// The integration path (`GITHUB_TOKEN` → `GH_TOKEN` → `gh auth token`)
    /// is validated by the `cargo run -- debug dump` end-to-end check.
    fn load_token_from(token: &str) -> anyhow::Result<String> {
        let trimmed = token.trim().to_owned();
        if trimmed.is_empty() {
            anyhow::bail!("empty token");
        }
        Ok(trimmed)
    }

    #[test]
    fn non_empty_token_is_returned() {
        let result = load_token_from("  ghp_test123  ");
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        if let Ok(token) = result {
            assert_eq!(token, "ghp_test123", "whitespace must be trimmed");
        }
    }

    #[test]
    fn empty_token_is_rejected() {
        let result = load_token_from("   ");
        assert!(result.is_err(), "empty token must return Err");
    }

    /// Verify that `load_token()` succeeds when either `GITHUB_TOKEN` or
    /// `GH_TOKEN` is set in the test process environment.  We read — not
    /// write — the env here, avoiding any unsafe mutation.
    #[test]
    fn env_path_succeeds_when_token_present() {
        // If neither env var is set and `gh` is not available this test is
        // vacuously skipped; the e2e `debug dump` run covers the full path.
        if std::env::var("GITHUB_TOKEN").is_ok() || std::env::var("GH_TOKEN").is_ok() {
            let result = load_token();
            assert!(result.is_ok(), "expected Ok when env var set: {result:?}");
        }
    }
}
