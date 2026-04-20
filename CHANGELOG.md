# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.7] — 2026-04-20

Patch release for keyboard layouts where `^` is a dead key.

### Fixed

- Added `C` as the primary non-dead-key shortcut for the Commits section.
  `^` / `ˆ` remain supported when the terminal sends them, but users no longer
  need to press a dead-key caret twice to reach Commits.
- Updated the status bar and help text to show `C` for Commits, avoiding hints
  that encourage the problematic dead-key path.

### Tests

290 pass: added coverage for `C` selecting the Commits section while preserving
the existing caret and Shift+6 shortcut paths.

## [0.2.6] — 2026-04-20

Patch release for Commits shortcut tolerance and commit-diff warmup visibility.

### Fixed

- `Shift+6` now handles terminals that report the key as `Char('6')` with
  additional modifier bits such as Alt or Ctrl, instead of requiring the
  modifier set to be exactly Shift.
- Additional caret-like characters emitted by some layouts are accepted for the
  Commits shortcut.

### Changed

- The Commits sidebar row now turns warning-colored while per-commit diffs are
  still warming and shows a compact ready/total marker.
- The Commits section and status bar now surface commit-diff warmup progress so
  a delayed per-commit diff fetch is visible instead of looking like a missed key.
- Commit-section scroll clamping now follows the same non-wrapping behavior as
  the Commits renderer.

### Tests

289 pass: added coverage for modified `Shift+6` key events and commit-diff
readiness counts.

## [0.2.5] — 2026-04-20

Patch release for commit-scoped diff reliability and back navigation.

### Fixed

- PR detail loads now eagerly prefetch per-commit diffs in the background and
  de-duplicate in-flight commit diff requests. Pressing `Enter` on a commit now
  reuses the same cache/pending path instead of spawning duplicate REST calls.
- Late failures from duplicate or background commit-diff requests no longer
  clear an already-cached, working scoped diff.
- Background PR-detail refreshes preserve the selected commit by SHA when that
  commit still exists after the refresh, avoiding surprise fallbacks to the
  cumulative HEAD diff.
- Restoring a cached tab clears unpersisted commit-scope state so it cannot get
  stuck on a `Fetching commit diff...` placeholder without a fetch in flight.
- `Esc` and `b` from a commit-scoped Files diff now return to the Commits list
  with the same commit highlighted; a second `Esc`/`b` still exits detail.

### Tests

287 pass: added coverage for scoped back navigation, refresh preservation,
late failure races, and cached-tab restore scope clearing.

## [0.2.4] — 2026-04-20

Patch release for commit-navigation ergonomics on non-US keyboards.

### Fixed

- `Shift+6` now accepts both ASCII `^` and the `ˆ` character emitted by some
  keyboard layouts, so the Commits section shortcut works with those terminals.
- Pressing `Enter` on a commit now scopes the commit and immediately opens the
  Files diff view for that commit. Previously it scoped the commit but left the
  user on the Commits list, which made it look like nothing useful happened.
- While a per-commit diff is still being fetched, the Files section now shows a
  loading placeholder instead of temporarily rendering the cumulative HEAD diff
  under a scoped banner.

### Tests

282 pass: added coverage for the `ˆ` Commits shortcut and the commit-Enter
state transition into Files diff mode.

## [0.2.3] — 2026-04-20

Patch release for PR-detail keyboard regressions found after the
commit-scope work.

### Fixed

- Detail section shortcuts (`!`, `@`, `#`, `$`, `%`, `^`) now run before the
  Ctrl/Alt modifier filter. This keeps section switching working on terminals
  and keyboard layouts that emit typed punctuation with AltGr/Option modifiers.
- Restoring a cached PR detail now rebuilds the review-thread index, so `t`
  can expand inline file comments after cache-backed navigation instead of
  becoming a silent no-op.
- Pressing `t` in a file diff now refreshes the render-derived thread cursor
  before toggling, and flashes when there is no review thread at the current
  diff position.

### Changed

- Help and status-bar hints now include `^` for the Commits section plus the
  `Enter`/`H` commit-scope flow.

### Tests

281 pass (was 279, +2 new): `modified_punctuation_still_selects_sections`,
`restored_pr_cache_rebuilds_thread_index_for_file_thread_shortcut`.

## [0.2.2] — 2026-04-20

Final slice of the three-patch Commits feature arc. Scoping a
commit now also filters the Comments section and annotates the
commit list rows with CI state.

### Added

- **Scoped Comments.** When `selected_commit.is_some()`, the
  Comments section filters review threads to those whose opening
  comment carries the selected commit's `originalCommit.oid`. A
  hint row `◈ Scoped to a3f7b2c · showing N of M threads · H
  returns to HEAD` in `palette.warning` makes the scope unmissable.
  Issue-level comments (PR-wide, never commit-scoped) still render
  under their own separator. Empty scopes render a muted "No
  review threads originated on this commit" notice so the user
  doesn't mistake an empty list for a bug.
- **Per-commit CI glyph** in the Commits list. Each row gains a
  CI-state glyph next to the author (via the same
  `glyphs::ci_glyph` + `palette.color_for(role)` helpers the
  Dashboard uses), so the user sees at a glance which commits
  passed, failed, or are still pending before they `Enter` to
  scope. Column drops first on 60-col narrow terminals.
- **"Showing last 100 commits" footer** on the Commits list when
  the PR has ≥100 commits. Resolves the "Known limitations" note
  from 0.2.0. Older commits still require a pagination system —
  deferred.

### Data layer

- **GraphQL delta**: each comment node on `reviewThreads` gains
  `originalCommit { oid }`; each commit node gains a minimal
  `statusCheckRollup { state }` (the HEAD commit's full rollup with
  `contexts` is unchanged — the Checks section still works as
  today). Cost: one scalar per comment (2000 for max-size PR),
  one scalar per commit (100 max). Negligible budget hit.
- `ReviewComment` gains `original_commit_id: Option<String>`.
- `ReviewThread` gains a `originating_commit_sha() -> Option<&str>`
  helper that returns the first comment's origin — the same
  promotion pattern used for `diff_hunk` in 0.1.5.
- `PrCommit` gains `check_state: Option<CheckState>` — reusing
  the existing enum from `crate::github::types`.

### Known limitations / non-goals

- The Checks **section** (the PR-wide context list) is NOT scoped
  per commit — it continues to show HEAD's rollup regardless of
  `selected_commit`. Scoping the full section would require a
  clear indicator AND answers to retry/rerun semantics that
  aren't needed today.
- No commit-list pagination beyond the "last 100" cap.

### Tests

279 pass (was 275, +4 new): `scoped_comments_filter_by_origin_commit`,
`scoped_comments_show_scope_hint`, `scoped_comments_empty_scope_shows_notice`,
`per_commit_ci_glyph_rendered_in_list`.

## [0.2.1] — 2026-04-20

Second slice of the three-patch Commits feature arc. The Commits
section shipped as a read-only list in 0.2.0 — now it's
interactive: selecting a commit re-scopes the Files section to
that commit's delta, a loud inline indicator makes the scoped
state unmissable, and `H` returns to HEAD from anywhere.

### Added

- **Commit selection + Files scoping.** `Enter` on the highlighted
  row in the Commits section fetches that commit's delta via
  `GET /repos/.../commits/{sha}` and routes the Files view
  through the scoped patches. Files the commit didn't touch are
  hidden from navigation while scoped.
- **`H` key** returns to HEAD from any section while
  `selected_commit.is_some()`. Flashes `"Returned to HEAD"`. No-op
  on HEAD so vim-style movement can land here later without
  conflict.
- **Inline scope strip** at the top of the right pane when
  scoped: `◈ Scoped to a3f7b2c — "feat: commit scope selector" ·
  H returns to HEAD` in `palette.warning` on a `help_bg` row.
  Absent entirely at HEAD — zero noise for the 100% case.
- **Status bar segment**: `◈ a3f7b2c · H→HEAD` in
  `palette.warning` when scoped. Complements the strip.
- **`j` / `k` / `gg` / `G`** navigate the commit list
  (`commits_cursor`). Standard vim motions; no new infrastructure.

### Data layer

- **`DetailCache::commit_patches`** — new `HashMap<(repo, sha),
  Cached<HashMap<path, Option<patch>>>>`. Same `FRESH_TTL` as
  existing caches.
- **Force-push defense.** On every fresh `PrDetail`, the cache
  prunes `commit_patches` entries whose SHA is absent from the
  new commit list. If `selected_commit` pointed at a removed
  index, it resets to `None` and flashes a notice.
- **REST fetch.** New `Client::fetch_commit_diff(repo, sha)`
  parses the response into the same `HashMap<path,
  Option<patch>>` shape the PR files path already uses. Binary
  files and oversized diffs preserve `None` just like today.

### Internal

- `App` gains `selected_commit: Option<usize>` and
  `commits_cursor: usize`. Both cleared in `clear_detail_state`.
- `Action::CommitDiffLoaded` + `Action::CommitDiffFailed` +
  `App::spawn_commit_diff_fetch` follow the existing
  `spawn_supervised_detail_fetch` + `send_or_warn` patterns.
- Expanded-thread state (`pr_detail_expanded_threads`,
  `pr_detail_diff_cursor`) clears on every `selected_commit`
  change — line anchors aren't valid across a scope switch.
  Documented inline as intentional UX, not a bug.
- Inline thread expansion (`t`/`T`) disabled while scoped. The
  thread hint line above the diff adjusts accordingly.

### Known limitations

- Comments are NOT filtered by commit yet — they render full
  PR-wide. 0.2.2 adds `original_commit_id`-based filtering.
- Per-commit `statusCheckRollup` isn't shown in the commit list
  rows. Also 0.2.2.

## [0.2.0] — 2026-04-20

First slice of a three-patch arc adding per-commit navigation to the
PR detail view. This release ships the **Commits section as a
read-only list**. Selection and commit-scoped Files (+ inline
indicator, `H` return-to-HEAD) ship in 0.2.1; scoped Comments and
per-commit CI ship in 0.2.2.

Minor version bump because it adds a new visible section (and a
field on `PrDetail`), not because it breaks anything. All existing
keys, flows, and cached data continue to work unchanged.

### Added

- **Commits section** (6th entry alongside Description / Checks /
  Reviews / Files / Comments). Lists the PR's most recent 100
  commits, newest first. Each row shows: short SHA (`palette.muted`),
  message headline (truncated), `@author` (`palette.dim`), relative
  age (`palette.dim`), `+additions` (`palette.git_new`),
  `−deletions` (`palette.danger`). Degrades gracefully at 60 cols.
- **`^` (Shift+6) keybinding** jumps to the Commits section,
  extending the existing `!@#$%` section sequence.
- Section hidden automatically for PRs with zero commits (via the
  existing `has_content()` predicate) and for every issue (issues
  have no `commits` field).

### Data layer

- **GraphQL delta:** `PR_DETAIL_QUERY` now fetches `commits(last:
  100)` with `oid`, `messageHeadline`, `author { name date }`,
  `additions`, `deletions`, `changedFilesIfAvailable`. The nested
  `statusCheckRollup` on the last commit is retained for the
  existing Checks section. Per-commit `statusCheckRollup` surfaces
  in 0.2.2.
- **Domain model:** new `PrCommit { sha, short_sha, headline,
  author, committed_at, additions, deletions, changed_files }`.
  `PrDetail` grows a `commits: Vec<PrCommit>` field, populated in
  `raw_pr_to_detail` and sorted descending by `committed_at` so
  `commits[0]` is always HEAD.

### Internal

- New `src/ui/pr_detail/commits.rs` renderer.
- `DetailSection::ALL` grows to length 6; `label()` and
  `has_content()` updated. Sidebar entry is automatic.
- `#![recursion_limit = "256"]` added to `src/main.rs` — the PR
  detail fixture's `serde_json::json!` tree depth grew past the
  default 128 once per-commit fields were added. No runtime impact.
- Thirteen existing `PrDetail { … }` literals across tests and the
  SWR cache gained a `commits: vec![]` stub to keep building.

### Known limitations

- No commit selection yet — that ships in 0.2.1.
- PRs with >100 commits show only the latest 100; a "showing last
  100" footer is deferred to 0.2.2.

## [0.1.11] — 2026-04-20

### Added

- **`t` / `T` in the help overlay.** The keybindings shipped in
  0.1.8 for inline thread expansion were only documented inside
  the collapsed cards themselves — dead text unless the user was
  already scrolled to an anchored line. They now appear in the
  `?` help overlay alongside `J`/`K` under the PR Detail section
  entries.
- **Thread hint line above the Files diff.** When the open file
  has any review threads, the header block gains a second hint
  line: `N threads · M unresolved  ·  [t] expand at cursor  ·
  [T] collapse all`. Shown in `palette.warning` when unresolved
  threads remain, `palette.muted` when all are resolved. Absent
  entirely on files with no threads so it doesn't stack dead UI.

Both fixes address a real 0.1.8 discoverability gap reported by
the user: pressing `t` in the Comments section (where it's a
no-op by design) with no hint anywhere that the key belonged to
the Files diff.

## [0.1.10] — 2026-04-20

Second pure internal refactor in a row — no user-visible change.

### Internal

- **`TextSink` enum in `src/ui/markdown.rs`.** The `Event::Text`
  arm inside `handle_event` previously routed incoming text via a
  three-way `if` chain on `code_block_lang.is_some()` / `in_table`
  / else. The state was implicit across three bool checks; adding
  a fourth context would be an easy omission.
- Introduce `TextSink { Inline, CodeBlock, TableCell }` and a
  `Builder::text_sink()` helper that computes the variant from
  existing state once per event. The `Event::Text` arm becomes an
  exhaustive 3-variant `match` — adding a fourth future context
  forces a compile error on both the enum and the arm.
- Cross-reference comment added above `Event::Text` and
  `Event::Code` explaining why `Code` has only two possible
  contexts (never a code block) and calling out that any new
  context must update both arms.
- Behaviour locked by three new snapshot tests committed BEFORE
  the refactor: `text_sink_snapshot_inline_paragraph`,
  `text_sink_snapshot_fenced_code_block`,
  `text_sink_snapshot_gfm_table`. They assert structural
  invariants (line counts, palette-coloured span counts, border-
  drawing glyphs present) rather than literal `Vec<Line>` equality
  — stable against palette tweaks, strict against state-machine
  drift.
- Net: +56 LoC on `src/ui/markdown.rs`. 267 tests pass (was 264,
  +3 snapshots).

## [0.1.9] — 2026-04-20

Pure internal refactor — no user-visible behaviour change. Zero
colour drift verified by a parity test run against every theme
before the legacy code was removed.

### Internal

- **`ThemeTokens` refactor of `src/theme.rs`.** Replace the single
  350-line `match theme` in `Palette::from_theme` with a
  `ThemeTokens` source-of-truth struct (7 direct colours + opt-out
  `Option<Color>` overrides for per-theme specials) plus a
  `Palette::from_tokens` that applies derivation rules and honours
  overrides. `ThemeTokens::for_theme(Theme)` becomes the only place
  each palette's identity lives. `Palette::from_theme` shrinks to
  a 2-line shim.
- **Parity gate.** Before the legacy code was removed, a new test
  asserts `Palette::from_tokens(ThemeTokens::for_theme(t)) ==
  Palette::from_theme(t)` for every `Theme` variant. This caught
  several rules that didn't hold universally — documented in the
  commit message — and every divergence was resolved by either
  promoting the field to a direct token or adding an opt-out
  override.
- **Contrast sanity test.** Adds `assert_ne!(border,
  border_focused)` across every theme, guarding against a future
  derivation rule accidentally producing identical colours on the
  focused-border hue. Locks in the memory-noted overlay-contrast
  invariant at the theme level.
- `Palette` gains `#[derive(PartialEq)]` (needed by the parity
  gate). No runtime cost — all fields are `Color` / `Copy`.
- Net: +~110 LoC on `src/theme.rs`. The refactor trades a 40-field
  match arm per theme for an opt-out override struct; future palette
  fields now only require editing the derivation rule or tagging
  themes that need overrides, not every arm.

## [0.1.8] — 2026-04-20

Phase B.2 — closes the review-thread UX story opened in 0.1.5.
When the user drills into a file's diff, threads anchored to
specific lines now expand inline at those lines, and file-level
or outdated threads collect in a labelled block at the bottom of
the file body.

### Added

- **Inline thread cards in the Files diff.** Each line that has
  active, non-outdated threads in the `ThreadIndex` gets a
  collapsed summary row immediately beneath it: `○ N threads ·
  M unresolved    [t] expand` in `palette.warning` when any
  thread on that anchor is unresolved, or `✔ N threads` in
  `palette.muted` when all are resolved. Glyph deliberately
  differs from the sidebar's `⚑` to avoid visual collision.
- **Expanded cards.** Pressing `t` on the cursor's anchor line
  replaces the collapsed summary with the full thread body —
  same gutter / author / markdown-body layout as the Comments
  section, indented to the content column so the diff's
  `+`/`-` gutter coloring on adjacent rows isn't disturbed. The
  expanded header row carries an inline `[t] collapse  [T]`
  hint.
- **Overflow block.** File-level threads (`line == None`) and
  outdated threads whose anchor line isn't in the current diff
  collect after the last hunk under a
  `╌╌ File-level & outdated threads (N) ╌╌` divider. Silent-
  drop of force-push orphaned threads is explicitly avoided.
- **Keybindings.** `t` toggles the thread at the diff cursor;
  `T` (Shift+t) collapses all expanded threads. Both guarded to
  the Files section in diff-mode; the help overlay documents
  them.

### Internal

- New `src/ui/pr_detail/thread_card.rs` with `render_thread_card`
  (collapsed / expanded dispatch) plus a `collapsed_summary_line`
  helper.
- New `render_diff_with_threads` in `src/ui/pr_detail/files.rs`
  — walks `file.hunks` directly so it has `DiffLine.new_lineno`
  natively (no post-hoc annotation of `render_diff` output); emits
  per-line thread cards from the `ThreadIndex` and writes the
  current anchor to `App::pr_detail_diff_cursor` each frame for
  the `t` key handler to read.
- `render_diff_line` promoted to `pub(crate)` so the new path
  can reuse the existing per-line renderer.
- `App` gains `pr_detail_expanded_threads: HashSet<(String, u32)>`
  and `pr_detail_diff_cursor: RefCell<Option<(String, u32)>>`.
  Both ephemeral; cleared alongside `thread_index` in
  `clear_detail_state` and on `back_to_dashboard`.
- `comments::render_thread_body` now `pub(super)` so the inline
  card can reuse the Comments section's visual language.
- `build_section` signature grows `expanded_threads` + `diff_cursor`
  parameters, plumbed from `App` at both render sites.

### Known limitations

- Multi-line review comment ranges (`startLine < line`) still
  render at `line` only. Deferred to 0.2.x.
- `T` preserves scroll position; no explicit "jump back to top
  after collapse-all" is implemented.

## [0.1.7] — 2026-04-20

Third of the review-thread UX patches (Phase B.1 — per-file
indicators). Phase B.2 (inline thread expansion in the diff) ships
in 0.1.8.

### Added

- **Per-file thread badges** in the Files overview and sidebar. Any
  file with review threads now carries a small indicator:
  `⚑ N` in `palette.warning` when the file has any unresolved
  non-outdated thread, `✓ N` in `palette.muted` when every thread
  is resolved or outdated. Sidebar variant omits the count to save
  columns and shows just the glyph. Files with zero threads stay
  unchanged, so the overview doesn't suddenly grow a column of
  check marks on PRs without review activity.
- New `ThreadIndex` in `src/ui/pr_detail/thread_index.rs`: a
  once-per-`PrDetail` lookup table keyed on `(path, line)` for
  the active-line bucket and `path` for the file-level / outdated
  overflow bucket. Per-file `total_for` and `unresolved_for`
  counters drive the badges. `active_at` and `overflow` accessors
  are present but unused until 0.1.8 (flagged `#[allow(dead_code)]`
  with a landing-commit note).

### Internal

- `App::thread_index: Option<ThreadIndex>` — rebuilt alongside
  `pr_detail` on every detail-loaded action; cleared in
  `clear_detail_state` so a brief race between a manual `r`
  refresh and the refetch can't leak a stale index.
- `build_section`, `build_files`, `build_files_overview`, and
  `sidebar_file_lines` all grow an `Option<&ThreadIndex>` parameter,
  plumbed through from `App::thread_index.as_ref()` at the render
  site. No behaviour change when the option is `None` — critical
  for PRs on older cached payloads.

## [0.1.6] — 2026-04-20

Second of three review-thread UX patches (Phase C — outdated
treatment). Next up: 0.1.7 adds per-file comment indicators in the
Files section and inline expansion of threads anchored to specific
diff lines.

### Added

- **ACTIVE / OUTDATED split in the Comments section.** Review
  threads are now partitioned under distinct dividers — a heavy
  `━━━ ACTIVE (N) ━━━` rule in `border_focused` and a dashed
  `╌╌╌ OUTDATED (N) ╌╌╌` rule in `muted`. Outdated threads still
  render (silent-drop is a documented TUI anti-pattern flagged by
  the research pass on octo.nvim) but every span inside them is
  re-coloured to `palette.muted` so they read as clearly
  subordinate to the active ones above.
- **`[OUTDATED]` badge** on the thread header for outdated
  threads, in `palette.danger` bold — so the state is visible at a
  glance rather than only via the trailing muted status word.
- **`z` keybind** toggles outdated-thread visibility. Default is
  shown-but-muted; press `z` to collapse to a single disclosure
  row `N outdated threads hidden · [z] show`. The divider stays
  visible even when collapsed so the presence of outdated threads
  is never hidden entirely. Help overlay documents the key.
- `n` / `N` (next/previous unresolved thread) now skip outdated
  threads entirely — navigation lands on open discussions, not
  stale ones.

### Internal

- Refactor `comments_lines` to partition threads via
  `Iterator::partition` and render each section through a shared
  closure, so the active/outdated passes can't drift visually.
- Extract `render_thread_body` (per-thread header + hunk + bodies)
  and `mute_lines` (fg override) helpers. The mute pass is lossy
  for syntax-highlighted outdated code blocks; documented as an
  intentional tradeoff.
- `App::detail_show_outdated: bool` (ephemeral, default `true`).

## [0.1.5] — 2026-04-20

First of three related feature drops around review-thread display
(0.1.5, 0.1.6, 0.1.7).

### Added

- **Inline diff-hunk excerpt in the Comments section.** Each review
  thread now renders the `diffHunk` GitHub captures at comment time
  as a small styled code excerpt — `@@ -a,b +c,d @@` header plus
  the surrounding context lines, coloured the same as the Files
  diff. Sits between the thread header and the first comment
  body, so readers no longer need to jump into the Files section
  to understand which code a review discusses. Capped at
  12 rendered rows; hunks longer than that show a muted
  `… hunk truncated` marker.

### Data layer

- GraphQL `PR_DETAIL_QUERY` adds `startLine`, `originalStartLine`
  on each review thread, and `diffHunk` on each review comment.
  Zero extra node-budget cost (all scalars). `startLine` is held
  on `ReviewThread` as groundwork for multi-line comment handling
  in 0.1.7; no UI surface for ranges in 0.1.5.
- `ReviewComment` gains `diff_hunk: Option<String>`.
  `ReviewThread` gains `start_line: Option<u32>` and
  `diff_hunk: Option<String>` (promoted from `comments[0].diff_hunk`
  at conversion time so renderers never reach into replies). All
  new fields are `#[serde(default)]` so session-cached `PrDetail`
  payloads from 0.1.4 deserialize without error and render
  without a hunk excerpt.

### Known limitations

- Multi-line review comments (where `startLine < line`) render at
  `line` only — the range isn't surfaced in the header yet. Tracked
  for 0.2.x.

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

[Unreleased]: https://github.com/leboiko/octopeek/compare/v0.2.7...HEAD
[0.2.7]: https://github.com/leboiko/octopeek/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/leboiko/octopeek/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/leboiko/octopeek/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/leboiko/octopeek/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/leboiko/octopeek/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/leboiko/octopeek/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/leboiko/octopeek/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/leboiko/octopeek/compare/v0.1.11...v0.2.0
[0.1.11]: https://github.com/leboiko/octopeek/compare/v0.1.10...v0.1.11
[0.1.10]: https://github.com/leboiko/octopeek/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/leboiko/octopeek/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/leboiko/octopeek/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/leboiko/octopeek/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/leboiko/octopeek/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/leboiko/octopeek/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/leboiko/octopeek/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/leboiko/octopeek/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/leboiko/octopeek/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/leboiko/octopeek/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/leboiko/octopeek/releases/tag/v0.1.0
