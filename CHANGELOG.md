# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.4] — 2026-04-20

### Fixed

- Diff rendering in the Files section no longer misaligns when a
  code line wraps. The PR-detail view enables `Wrap { trim: false }`
  on the final `Paragraph`, which is right for prose sections but
  wrong for a two-column-gutter diff: ratatui's word-wrapper drops
  each wrapped continuation to column 0, stomping on the `old-lineno
  new-lineno` gutter and making long tokens like
  `createFollowParityChecker)` visually misaligned with the sign
  column. Wrap now stays **off** for the Files section — same
  convention as GitHub's diff view, VS Code, and every other code
  diff viewer. Prose sections (Description, Checks, Reviews,
  Comments) keep their wrap-on behavior.
- `clamp_pr_detail_scroll` mirrors the render-side wrap decision:
  the Files section counts rendered rows as `lines.len()` (no
  wrap); every other section counts wrapped rows via
  `Paragraph::line_count`. Previously the clamp over-estimated for
  Files, leaving a stretch of empty rows below the last diff line
  when scrolling to the bottom.

### Known limitations

- Long diff lines clip at the right edge of the pane. Horizontal
  scrolling on diffs is a follow-up (0.1.5).

## [0.1.3] — 2026-04-20

### Fixed

- Entering copy mode (`v`) in the PR or issue detail view no
  longer collapses each logical line to a single display row. The
  copy-mode paragraph was being built without `Wrap`, trading the
  word-wrapper for a horizontal-scroll + stable-coordinate design;
  in practice that turned long rendered comments into what looked
  like 1-liners the moment selection was entered. Wrap is now on
  for both normal and copy-mode branches. `copy_mode::apply_overlay`
  still runs before the word-wrapper, so highlighted characters
  follow the wrap onto whichever rendered row they end up on.

### Internal

- Collapse the two branches of the final `Paragraph` build in
  `pr_detail::draw` and `issue_detail::draw` into one shared widget
  with a conditional line-transformation up front. Eliminates a
  small divergence between the two render paths.

## [0.1.2] — 2026-04-19

### Fixed

- Clicking a PR (or pressing `Enter` / `o` / `y` on the highlighted
  row) now opens the PR that was visually selected. The dashboard
  rendered PRs sorted by `Role → updated_at → number` while the
  click-resolution path looked up items in raw inbox order, so row N
  on screen resolved to a different PR in mixed-role cases. The
  mismatch was latent in 0.1.0 for `Enter` and worsened in 0.1.1
  where `o` / `y` on the dashboard inherited the same bug through
  `dashboard_selected_url`.

### Changed

- Dashboard sort simplified: PRs and issues both order strictly by
  `updated_at desc` with `number` as a deterministic tiebreaker. The
  previous Author → Reviewer → Assignee role-priority order is gone.

### Internal

- Extract `sorted_prs_for_repo` / `sorted_issues_for_repo` in
  `github::types`. Dashboard render, `open_detail_for_selection`,
  `CheckoutBranch` fallback, and `dashboard_selected_url` all call
  these helpers so the display order and the click-resolution order
  can never drift apart again.
- Regression test `dashboard_selection_opens_displayed_pr` asserts
  the most-recently-updated PR surfaces at row 0 regardless of raw
  inbox ordering.

## [0.1.1] — 2026-04-19

### Fixed

- Scroll clamp in the PR / issue detail view now counts **wrapped**
  rendered rows, not input lines. A long comment — or any body longer
  than the right-pane width — previously left its tail below the
  viewport floor with no way to scroll to it. The bug was latent from
  the original clamp in 82f8719; it became visible once the sidebar
  landed (a76aab6) and narrowed the right pane, triggering more
  aggressive word-wrap.

### Internal

- Opt into the `ratatui` `unstable-rendered-line-info` feature so we
  can call `Paragraph::line_count(width)`. Feature name is stable;
  only the function signature is subject to change. Tracked upstream
  at https://github.com/ratatui/ratatui/issues/293.
- New regression test `scroll_clamp_accounts_for_line_wrap` asserts a
  single 500-char body line in a 40-column viewport produces a
  non-zero `max_scroll`.

## [0.1.0] — 2026-04-19

First public release on crates.io. Install with `cargo install octopeek`.

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

### Added — pre-release hardening pass

- Manual `Debug` impl on the GitHub HTTP client that redacts the bearer token.
- `open_url_in_browser` now rejects any URL that doesn't start with `https://`,
  refusing `file://`, `ssh://`, and other schemes that could dispatch native
  commands on macOS / Linux.
- Malformed `config.toml` / session TOML now logs a `warn!` with the parse
  error before falling back to defaults, instead of silently resetting every
  user setting.
- `[deleted]` sentinel replaces the empty-string author rendering for GitHub
  accounts that have been deleted or suspended.
- Crate-level `//!` documentation and `#![warn(missing_docs)]` lint.

### Changed — pre-release refactor pass

- Monolithic `src/app/mod.rs` (4687 LoC) split into nine focused submodules.
- Monolithic `src/ui/pr_detail.rs` (2138 LoC) split into eight per-section
  modules.
- GraphQL `PullRequestFields` / `IssueFields` fragments extracted as shared
  `const` strings; both query builders now format from one source of truth.
- Generic `GqlEnvelope<T>` and `Client::post_graphql<B, T>` helper collapse
  three copies of the HTTP-plumbing code into one.
- `spawn_detail_fetch` + `spawn_detail_fetch_background` now share
  `spawn_supervised_detail_fetch`; the `tx.send` calls log a warn on a
  closed channel instead of silently dropping the result.
- `restore_detail_kind` collapses its PR and issue arms into one SWR flow.
- GraphQL raw types downgraded from `pub` to `pub(super)` / `pub(crate)` —
  the crate is a binary and should not expose implementation details.

[Unreleased]: https://github.com/leboiko/octopeek/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/leboiko/octopeek/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/leboiko/octopeek/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/leboiko/octopeek/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/leboiko/octopeek/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/leboiko/octopeek/releases/tag/v0.1.0

