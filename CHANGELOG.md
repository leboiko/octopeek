# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added (Phase 5)

- **Repo picker overlay** (`p` key): full-screen modal for adding and removing
  watched repositories.  List mode (`j`/`k` navigate, `d`/`Backspace` delete,
  `Enter` focuses that repo's tab, `Esc` closes).  Input mode (`a`/`i` enters,
  `Enter` validates and commits, `Esc` returns to List mode).
- **Branch checkout flow** (`c` key on dashboard or detail view): collects the
  PR's `head_ref` from either the list-level inbox data or the open detail view,
  shows a confirmation overlay (`[y] yes  [N] no/cancel`), then runs
  `git checkout <branch>` in the current working directory.
- `src/git.rs` — synchronous git helpers: `repo_cwd_is_git`,
  `is_working_tree_clean`, `current_branch`, `checkout_branch`.
- `src/ui/confirm.rs` — generic confirmation overlay with extensible
  `ConfirmPending` enum (currently: `CheckoutBranch`; designed to accommodate
  future actions such as "Confirm merge" or "Confirm close").
- `headRefName` / `baseRefName` added to the inbox GraphQL `PullRequestFields`
  fragment and the `PullRequest` domain type so branch checkout works from the
  dashboard without a separate detail fetch.
- `Focus::Confirm` and `RepoPickerMode` variants wired throughout the key
  dispatch, status bar, and UI render loop.
- 22 new unit tests (108 total, up from 86).

### Added (Phase 1–4 — carried over)

- Initial project scaffold.
- Async event loop built on `tokio` + a blocking `crossterm` polling thread.
- RAII `TerminalGuard` that restores the terminal on exit, error, and panic.
- Palette-based theme system with eight built-in themes: Default, Dracula, Solarized Dark, Solarized Light, Nord, Gruvbox Dark, Gruvbox Light, GitHub Light.
- Tab bar with stable `TabId`s, repo deduplication, and a `MAX_TABS` cap.
- XDG-based configuration (`~/.config/octopeek/config.toml`) and session persistence with graceful fallback on parse errors.
- Full `Action` enum and key-binding dispatcher covering every Phase 2–5 interaction (unimplemented variants log at `warn` level).
- Help overlay (`?`) listing every key binding.
- `clap`-powered CLI with `--version` and `--help`.
- CI pipeline (fmt, clippy, test) on Linux and macOS, stable and beta.
- Release workflow skeleton driven by git tags.
- OSS project hygiene: dual MIT/Apache-2.0 licensing, Contributor Covenant, security policy, contributing guide, issue and PR templates, CODEOWNERS, Dependabot.

<!-- Comparison links will be populated once the first version is tagged. -->

